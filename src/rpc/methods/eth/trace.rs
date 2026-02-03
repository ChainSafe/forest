// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::types::{
    EthAddress, EthBytes, EthCallTraceAction, EthHash, EthTrace, TraceAction, TraceResult,
};
use super::utils::{decode_params, decode_return};
use super::{
    EthCallTraceResult, EthCreateTraceAction, EthCreateTraceResult, decode_payload,
    encode_filecoin_params_as_abi, encode_filecoin_returns_as_abi,
};
use crate::eth::{EAMMethod, EVMMethod};
use crate::rpc::eth::types::{AccountDiff, Delta, StateDiff};
use crate::rpc::eth::{EthBigInt, EthUint64};
use crate::rpc::methods::eth::lookup_eth_address;
use crate::rpc::methods::state::ExecutionTrace;
use crate::rpc::state::ActorTrace;
use crate::shim::actors::{EVMActorStateLoad, evm};
use crate::shim::fvm_shared_latest::METHOD_CONSTRUCTOR;
use crate::shim::state_tree::ActorState;
use crate::shim::{actors::is_evm_actor, address::Address, error::ExitCode, state_tree::StateTree};
use ahash::{HashMap, HashSet};
use anyhow::{Context, bail};
use fil_actor_eam_state::v12 as eam12;
use fil_actor_evm_state::evm_shared::v17::uints::U256;
use fil_actor_evm_state::v15 as evm12;
use fil_actor_init_state::v12::ExecReturn;
use fil_actor_init_state::v15::Method as InitMethod;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_kamt::{AsHashedKey, Config as KamtConfig, HashedKey, Kamt};
use num::FromPrimitive;
use std::borrow::Cow;
use std::collections::BTreeMap;
use tracing::debug;

/// KAMT configuration matching the EVM actor in builtin-actors.
// Code is taken from: https://github.com/filecoin-project/builtin-actors/blob/v17.0.0/actors/evm/src/interpreter/system.rs#L47
fn evm_kamt_config() -> KamtConfig {
    KamtConfig {
        bit_width: 5,       // 32 children per node (2^5)
        min_data_depth: 0,  // Data can be stored at root level
        max_array_width: 1, // Max 1 key-value pair per bucket
    }
}

/// Hash algorithm for EVM storage KAMT.
// Code taken from: https://github.com/filecoin-project/builtin-actors/blob/v17.0.0/actors/evm/src/interpreter/system.rs#L49.
pub struct EvmStateHashAlgorithm;

impl AsHashedKey<U256, 32> for EvmStateHashAlgorithm {
    fn as_hashed_key(key: &U256) -> Cow<'_, HashedKey<32>> {
        Cow::Owned(key.to_big_endian())
    }
}

/// Type alias for EVM storage KAMT with configuration.
type EvmStorageKamt<BS> = Kamt<BS, U256, U256, EvmStateHashAlgorithm>;

fn u256_to_eth_hash(value: &U256) -> EthHash {
    EthHash(ethereum_types::H256(value.to_big_endian()))
}

const ZERO_HASH: EthHash = EthHash(ethereum_types::H256([0u8; 32]));

#[derive(Default)]
pub struct Environment {
    caller: EthAddress,
    is_evm: bool,
    subtrace_count: i64,
    pub traces: Vec<EthTrace>,
    last_byte_code: Option<EthAddress>,
}

pub fn base_environment<BS: Blockstore + Send + Sync>(
    state: &StateTree<BS>,
    from: &Address,
) -> anyhow::Result<Environment> {
    let sender = lookup_eth_address(from, state)?
        .with_context(|| format!("top-level message sender {from} s could not be found"))?;
    Ok(Environment {
        caller: sender,
        ..Environment::default()
    })
}

fn trace_to_address(trace: &ActorTrace) -> EthAddress {
    if let Some(addr) = trace.state.delegated_address
        && let Ok(eth_addr) = EthAddress::from_filecoin_address(&addr.into())
    {
        return eth_addr;
    }
    EthAddress::from_actor_id(trace.id)
}

/// Returns true if the trace is a call to an EVM or EAM actor.
fn trace_is_evm_or_eam(trace: &ExecutionTrace) -> bool {
    if let Some(invoked_actor) = &trace.invoked_actor {
        is_evm_actor(&invoked_actor.state.code)
            || invoked_actor.id != Address::ETHEREUM_ACCOUNT_MANAGER_ACTOR.id().unwrap()
    } else {
        false
    }
}

/// Returns true if the trace is a call to an EVM or EAM actor.
fn trace_err_msg(trace: &ExecutionTrace) -> Option<String> {
    let code = trace.msg_rct.exit_code;

    if code.is_success() {
        return None;
    }

    // EVM tools often expect this literal string.
    if code == ExitCode::SYS_OUT_OF_GAS {
        return Some("out of gas".into());
    }

    // indicate when we have a "system" error.
    if code < ExitCode::FIRST_ACTOR_ERROR_CODE.into() {
        return Some(format!("vm error: {code}"));
    }

    // handle special exit codes from the EVM/EAM.
    if trace_is_evm_or_eam(trace) {
        match code.into() {
            evm12::EVM_CONTRACT_REVERTED => return Some("Reverted".into()), // capitalized for compatibility
            evm12::EVM_CONTRACT_INVALID_INSTRUCTION => return Some("invalid instruction".into()),
            evm12::EVM_CONTRACT_UNDEFINED_INSTRUCTION => {
                return Some("undefined instruction".into());
            }
            evm12::EVM_CONTRACT_STACK_UNDERFLOW => return Some("stack underflow".into()),
            evm12::EVM_CONTRACT_STACK_OVERFLOW => return Some("stack overflow".into()),
            evm12::EVM_CONTRACT_ILLEGAL_MEMORY_ACCESS => {
                return Some("illegal memory access".into());
            }
            evm12::EVM_CONTRACT_BAD_JUMPDEST => return Some("invalid jump destination".into()),
            evm12::EVM_CONTRACT_SELFDESTRUCT_FAILED => return Some("self destruct failed".into()),
            _ => (),
        }
    }
    // everything else...
    Some(format!("actor error: {code}"))
}

/// Recursively builds the traces for a given ExecutionTrace by walking the subcalls
pub fn build_traces(
    env: &mut Environment,
    address: &[i64],
    trace: ExecutionTrace,
) -> anyhow::Result<()> {
    let (trace, recurse_into) = build_trace(env, address, trace)?;

    let last_trace_idx = if let Some(trace) = trace {
        let len = env.traces.len();
        env.traces.push(trace);
        env.subtrace_count += 1;
        Some(len)
    } else {
        None
    };

    // Skip if there's nothing more to do and/or `build_trace` told us to skip this one.
    let (recurse_into, invoked_actor) = if let Some(trace) = recurse_into {
        if let Some(invoked_actor) = &trace.invoked_actor {
            let invoked_actor = invoked_actor.clone();
            (trace, invoked_actor)
        } else {
            return Ok(());
        }
    } else {
        return Ok(());
    };

    let mut sub_env = Environment {
        caller: trace_to_address(&invoked_actor),
        is_evm: is_evm_actor(&invoked_actor.state.code),
        traces: env.traces.clone(),
        ..Environment::default()
    };
    for subcall in recurse_into.subcalls.into_iter() {
        let mut new_address = address.to_vec();
        new_address.push(sub_env.subtrace_count);
        build_traces(&mut sub_env, &new_address, subcall)?;
    }
    env.traces = sub_env.traces;
    if let Some(idx) = last_trace_idx {
        env.traces.get_mut(idx).expect("Infallible").subtraces = sub_env.subtrace_count;
    }

    Ok(())
}

// `build_trace` processes the passed execution trace and updates the environment, if necessary.
//
// On success, it returns a trace to add (or `None` to skip) and the trace to recurse into (or `None` to skip).
fn build_trace(
    env: &mut Environment,
    address: &[i64],
    trace: ExecutionTrace,
) -> anyhow::Result<(Option<EthTrace>, Option<ExecutionTrace>)> {
    // This function first assumes that the call is a "native" call, then handles all the "not
    // native" cases. If we get any unexpected results in any of these special cases, we just
    // keep the "native" interpretation and move on.
    //
    // 1. If we're invoking a contract (even if the caller is a native account/actor), we
    //    attempt to decode the params/return value as a contract invocation.
    // 2. If we're calling the EAM and/or init actor, we try to treat the call as a CREATE.
    // 3. Finally, if the caller is an EVM smart contract and it's calling a "private" (1-1023)
    //    method, we know something special is going on. We look for calls related to
    //    DELEGATECALL and drop everything else (everything else includes calls triggered by,
    //    e.g., EXTCODEHASH).

    // If we don't have sufficient funds, or we have a fatal error, or we have some
    // other syscall error: skip the entire trace to mimic Ethereum (Ethereum records
    // traces _after_ checking things like this).
    //
    // NOTE: The FFI currently folds all unknown syscall errors into "sys assertion
    // failed" which is turned into SysErrFatal.
    if !address.is_empty()
        && Into::<ExitCode>::into(trace.msg_rct.exit_code) == ExitCode::SYS_INSUFFICIENT_FUNDS
    {
        return Ok((None, None));
    }

    // We may fail before we can even invoke the actor. In that case, we have no 100% reliable
    // way of getting its address (e.g., due to reverts) so we're just going to drop the entire
    // trace. This is OK (ish) because the call never really "happened".
    if trace.invoked_actor.is_none() {
        return Ok((None, None));
    }

    // Step 2: Decode as a contract invocation
    //
    // Normal EVM calls. We don't care if the caller/receiver are actually EVM actors, we only
    // care if the call _looks_ like an EVM call. If we fail to decode it as an EVM call, we
    // fallback on interpreting it as a native call.
    let method = EVMMethod::from_u64(trace.msg.method);
    if let Some(EVMMethod::InvokeContract) = method {
        let (trace, exec_trace) = trace_evm_call(env, address, trace)?;
        return Ok((Some(trace), Some(exec_trace)));
    }

    // Step 3: Decode as a contract deployment
    match trace.msg.to {
        Address::INIT_ACTOR => {
            let method = InitMethod::from_u64(trace.msg.method);
            match method {
                Some(InitMethod::Exec) | Some(InitMethod::Exec4) => {
                    return trace_native_create(env, address, &trace);
                }
                _ => (),
            }
        }
        Address::ETHEREUM_ACCOUNT_MANAGER_ACTOR => {
            let method = EAMMethod::from_u64(trace.msg.method);
            match method {
                Some(EAMMethod::Create)
                | Some(EAMMethod::Create2)
                | Some(EAMMethod::CreateExternal) => {
                    return trace_eth_create(env, address, &trace);
                }
                _ => (),
            }
        }
        _ => (),
    }

    // Step 4: Handle DELEGATECALL
    //
    // EVM contracts cannot call methods in the range 1-1023, only the EVM itself can. So, if we
    // see a call in this range, we know it's an implementation detail of the EVM and not an
    // explicit call by the user.
    //
    // While the EVM calls several methods in this range (some we've already handled above with
    // respect to the EAM), we only care about the ones relevant DELEGATECALL and can _ignore_
    // all the others.
    if env.is_evm && trace.msg.method > 0 && trace.msg.method < 1024 {
        return trace_evm_private(env, address, &trace);
    }

    Ok((Some(trace_native_call(env, address, &trace)?), Some(trace)))
}

// Build an EthTrace for a "call" with the given input & output.
fn trace_call(
    env: &mut Environment,
    address: &[i64],
    trace: &ExecutionTrace,
    input: EthBytes,
    output: EthBytes,
) -> anyhow::Result<EthTrace> {
    if let Some(invoked_actor) = &trace.invoked_actor {
        let to = trace_to_address(invoked_actor);
        let call_type: String = if trace.msg.read_only.unwrap_or_default() {
            "staticcall"
        } else {
            "call"
        }
        .into();

        Ok(EthTrace {
            r#type: "call".into(),
            action: TraceAction::Call(EthCallTraceAction {
                call_type,
                from: env.caller,
                to: Some(to),
                gas: trace.msg.gas_limit.unwrap_or_default().into(),
                value: trace.msg.value.clone().into(),
                input,
            }),
            result: TraceResult::Call(EthCallTraceResult {
                gas_used: trace.sum_gas().total_gas.into(),
                output,
            }),
            trace_address: Vec::from(address),
            error: trace_err_msg(trace),
            ..EthTrace::default()
        })
    } else {
        bail!("no invoked actor")
    }
}

// Build an EthTrace for a "call", parsing the inputs & outputs as a "native" FVM call.
fn trace_native_call(
    env: &mut Environment,
    address: &[i64],
    trace: &ExecutionTrace,
) -> anyhow::Result<EthTrace> {
    trace_call(
        env,
        address,
        trace,
        encode_filecoin_params_as_abi(trace.msg.method, trace.msg.params_codec, &trace.msg.params)?,
        EthBytes(encode_filecoin_returns_as_abi(
            trace.msg_rct.exit_code.value().into(),
            trace.msg_rct.return_codec,
            &trace.msg_rct.r#return,
        )),
    )
}

// Build an EthTrace for a "call", parsing the inputs & outputs as an EVM call (falling back on
// treating it as a native call).
fn trace_evm_call(
    env: &mut Environment,
    address: &[i64],
    trace: ExecutionTrace,
) -> anyhow::Result<(EthTrace, ExecutionTrace)> {
    let input = match decode_payload(&trace.msg.params, trace.msg.params_codec) {
        Ok(value) => value,
        Err(err) => {
            debug!("failed to decode contract invocation payload: {err}");
            return Ok((trace_native_call(env, address, &trace)?, trace));
        }
    };
    let output = match decode_payload(&trace.msg_rct.r#return, trace.msg_rct.return_codec) {
        Ok(value) => value,
        Err(err) => {
            debug!("failed to decode contract invocation return: {err}");
            return Ok((trace_native_call(env, address, &trace)?, trace));
        }
    };
    Ok((trace_call(env, address, &trace, input, output)?, trace))
}

// Build an EthTrace for a native "create" operation. This should only be called with an
// ExecutionTrace is an Exec or Exec4 method invocation on the Init actor.

fn trace_native_create(
    env: &mut Environment,
    address: &[i64],
    trace: &ExecutionTrace,
) -> anyhow::Result<(Option<EthTrace>, Option<ExecutionTrace>)> {
    if trace.msg.read_only.unwrap_or_default() {
        // "create" isn't valid in a staticcall, so we just skip this trace
        // (couldn't have created an actor anyways).
        // This mimic's the EVM: it doesn't trace CREATE calls when in
        // read-only mode.
        return Ok((None, None));
    }

    let sub_trace = trace
        .subcalls
        .iter()
        .find(|c| c.msg.method == METHOD_CONSTRUCTOR);

    let sub_trace = if let Some(sub_trace) = sub_trace {
        sub_trace
    } else {
        // If we succeed in calling Exec/Exec4 but don't even try to construct
        // something, we have a bug in our tracing logic or a mismatch between our
        // tracing logic and the actors.
        if trace.msg_rct.exit_code.is_success() {
            bail!("successful Exec/Exec4 call failed to call a constructor");
        }
        // Otherwise, this can happen if creation fails early (bad params,
        // out of gas, contract already exists, etc.). The EVM wouldn't
        // trace such cases, so we don't either.
        //
        // NOTE: It's actually impossible to run out of gas before calling
        // initcode in the EVM (without running out of gas in the calling
        // contract), but this is an equivalent edge-case to InvokedActor
        // being nil, so we treat it the same way and skip the entire
        // operation.
        return Ok((None, None));
    };

    // Native actors that aren't the EAM can attempt to call Exec4, but such
    // call should fail immediately without ever attempting to construct an
    // actor. I'm catching this here because it likely means that there's a bug
    // in our trace-conversion logic.
    if trace.msg.method == (InitMethod::Exec4 as u64) {
        bail!("direct call to Exec4 successfully called a constructor!");
    }

    let mut output = EthBytes::default();
    let mut create_addr = EthAddress::default();
    if trace.msg_rct.exit_code.is_success() {
        // We're supposed to put the "installed bytecode" here. But this
        // isn't an EVM actor, so we just put some invalid bytecode (this is
        // the answer you'd get if you called EXTCODECOPY on a native
        // non-account actor, anyways).
        output = EthBytes(vec![0xFE]);

        // Extract the address of the created actor from the return value.
        let init_return: ExecReturn = decode_return(&trace.msg_rct)?;
        let actor_id = init_return.id_address.id()?;
        let eth_addr = EthAddress::from_actor_id(actor_id);
        create_addr = eth_addr;
    }

    Ok((
        Some(EthTrace {
            r#type: "create".into(),
            action: TraceAction::Create(EthCreateTraceAction {
                from: env.caller,
                gas: trace.msg.gas_limit.unwrap_or_default().into(),
                value: trace.msg.value.clone().into(),
                // If we get here, this isn't a native EVM create. Those always go through
                // the EAM. So we have no "real" initcode and must use the sentinel value
                // for "invalid" initcode.
                init: EthBytes(vec![0xFE]),
            }),
            result: TraceResult::Create(EthCreateTraceResult {
                gas_used: trace.sum_gas().total_gas.into(),
                address: Some(create_addr),
                code: output,
            }),
            trace_address: Vec::from(address),
            error: trace_err_msg(trace),
            ..EthTrace::default()
        }),
        Some(sub_trace.clone()),
    ))
}

// Decode the parameters and return value of an EVM smart contract creation through the EAM. This
// should only be called with an ExecutionTrace for a Create, Create2, or CreateExternal method
// invocation on the EAM.
fn decode_create_via_eam(trace: &ExecutionTrace) -> anyhow::Result<(Vec<u8>, EthAddress)> {
    let init_code = match EAMMethod::from_u64(trace.msg.method) {
        Some(EAMMethod::Create) => {
            let params = decode_params::<eam12::CreateParams>(&trace.msg)?;
            params.initcode
        }
        Some(EAMMethod::Create2) => {
            let params = decode_params::<eam12::Create2Params>(&trace.msg)?;
            params.initcode
        }
        Some(EAMMethod::CreateExternal) => {
            decode_payload(&trace.msg.params, trace.msg.params_codec)?.into()
        }
        _ => bail!("unexpected CREATE method {}", trace.msg.method),
    };
    let ret = decode_return::<eam12::CreateReturn>(&trace.msg_rct)?;

    Ok((init_code, ret.eth_address.0.into()))
}

// Build an EthTrace for an EVM "create" operation. This should only be called with an
// ExecutionTrace for a Create, Create2, or CreateExternal method invocation on the EAM.
fn trace_eth_create(
    env: &mut Environment,
    address: &[i64],
    trace: &ExecutionTrace,
) -> anyhow::Result<(Option<EthTrace>, Option<ExecutionTrace>)> {
    // Same as the Init actor case above, see the comment there.
    if trace.msg.read_only.unwrap_or_default() {
        return Ok((None, None));
    }

    // Look for a call to either a constructor or the EVM's resurrect method.
    let sub_trace = trace
        .subcalls
        .iter()
        .filter_map(|et| {
            if et.msg.to == Address::INIT_ACTOR {
                et.subcalls
                    .iter()
                    .find(|et| et.msg.method == METHOD_CONSTRUCTOR)
            } else {
                match EVMMethod::from_u64(et.msg.method) {
                    Some(EVMMethod::Resurrect) => Some(et),
                    _ => None,
                }
            }
        })
        .next();

    // Same as the Init actor case above, see the comment there.
    let sub_trace = if let Some(sub_trace) = sub_trace {
        sub_trace
    } else {
        if trace.msg_rct.exit_code.is_success() {
            bail!("successful Create/Create2 call failed to call a constructor");
        }
        return Ok((None, None));
    };

    // Decode inputs & determine create type.
    let (init_code, create_addr) = decode_create_via_eam(trace)?;

    // Handle the output.
    let output = match trace.msg_rct.exit_code.value() {
        0 => {
            // success
            // We're _supposed_ to include the contracts bytecode here, but we
            // can't do that reliably (e.g., if some part of the trace reverts).
            // So we don't try and include a sentinel "impossible bytecode"
            // value (the value specified by EIP-3541).
            EthBytes(vec![0xFE])
        }
        33 => {
            // Reverted, parse the revert message.
            // If we managed to call the constructor, parse/return its revert message. If we
            // fail, we just return no output.
            decode_payload(&sub_trace.msg_rct.r#return, sub_trace.msg_rct.return_codec)?
        }
        _ => EthBytes::default(),
    };

    Ok((
        Some(EthTrace {
            r#type: "create".into(),
            action: TraceAction::Create(EthCreateTraceAction {
                from: env.caller,
                gas: trace.msg.gas_limit.unwrap_or_default().into(),
                value: trace.msg.value.clone().into(),
                init: init_code.into(),
            }),
            result: TraceResult::Create(EthCreateTraceResult {
                gas_used: trace.sum_gas().total_gas.into(),
                address: Some(create_addr),
                code: output,
            }),
            trace_address: Vec::from(address),
            error: trace_err_msg(trace),
            ..EthTrace::default()
        }),
        Some(sub_trace.clone()),
    ))
}

// Build an EthTrace for a "private" method invocation from the EVM. This should only be called with
// an ExecutionTrace from an EVM instance and on a method between 1 and 1023 inclusive.
fn trace_evm_private(
    env: &mut Environment,
    address: &[i64],
    trace: &ExecutionTrace,
) -> anyhow::Result<(Option<EthTrace>, Option<ExecutionTrace>)> {
    // The EVM actor implements DELEGATECALL by:
    //
    // 1. Asking the callee for its bytecode by calling it on the GetBytecode method.
    // 2. Recursively invoking the currently executing contract on the
    //    InvokeContractDelegate method.
    //
    // The code below "reconstructs" that delegate call by:
    //
    // 1. Remembering the last contract on which we called GetBytecode.
    // 2. Treating the contract invoked in step 1 as the DELEGATECALL receiver.
    //
    // Note, however: GetBytecode will be called, e.g., if the user invokes the
    // EXTCODECOPY instruction. It's not an error to see multiple GetBytecode calls
    // before we see an InvokeContractDelegate.
    match EVMMethod::from_u64(trace.msg.method) {
        Some(EVMMethod::GetBytecode) => {
            // NOTE: I'm not checking anything about the receiver here. The EVM won't
            // DELEGATECALL any non-EVM actor, but there's no need to encode that fact
            // here in case we decide to loosen this up in the future.
            env.last_byte_code = None;
            if trace.msg_rct.exit_code.is_success()
                && let Option::Some(actor_trace) = &trace.invoked_actor
            {
                let to = trace_to_address(actor_trace);
                env.last_byte_code = Some(to);
            }
            Ok((None, None))
        }
        Some(EVMMethod::InvokeContractDelegate) => {
            // NOTE: We return errors in all the failure cases below instead of trying
            // to continue because the caller is an EVM actor. If something goes wrong
            // here, there's a bug in our EVM implementation.

            // Handle delegate calls
            //
            // 1) Look for trace from an EVM actor to itself on InvokeContractDelegate,
            //    method 6.
            // 2) Check that the previous trace calls another actor on method 3
            //    (GetByteCode) and they are at the same level (same parent)
            // 3) Treat this as a delegate call to actor A.
            if env.last_byte_code.is_none() {
                bail!("unknown bytecode for delegate call");
            }

            if let Option::Some(actor_trace) = &trace.invoked_actor {
                let to = trace_to_address(actor_trace);
                if env.caller != to {
                    bail!(
                        "delegate-call not from address to self: {:?} != {:?}",
                        env.caller,
                        to
                    );
                }
            }

            let dp = decode_params::<evm12::DelegateCallParams>(&trace.msg)?;

            let output = decode_payload(&trace.msg_rct.r#return, trace.msg_rct.return_codec)
                .map_err(|e| anyhow::anyhow!("failed to decode delegate-call return: {}", e))?;

            Ok((
                Some(EthTrace {
                    r#type: "call".into(),
                    action: TraceAction::Call(EthCallTraceAction {
                        call_type: "delegatecall".into(),
                        from: env.caller,
                        to: env.last_byte_code,
                        gas: trace.msg.gas_limit.unwrap_or_default().into(),
                        value: trace.msg.value.clone().into(),
                        input: dp.input.into(),
                    }),
                    result: TraceResult::Call(EthCallTraceResult {
                        gas_used: trace.sum_gas().total_gas.into(),
                        output,
                    }),
                    trace_address: Vec::from(address),
                    error: trace_err_msg(trace),
                    ..EthTrace::default()
                }),
                Some(trace.clone()),
            ))
        }
        _ => {
            // We drop all other "private" calls from FEVM. We _forbid_ explicit calls between 0 and
            // 1024 (exclusive), so any calls in this range must be implementation details.
            Ok((None, None))
        }
    }
}

/// Build state diff by comparing pre and post-execution states for touched addresses.
pub(crate) fn build_state_diff<S: Blockstore, T: Blockstore>(
    store: &S,
    pre_state: &StateTree<T>,
    post_state: &StateTree<T>,
    touched_addresses: &HashSet<EthAddress>,
) -> anyhow::Result<StateDiff> {
    let mut state_diff = StateDiff::new();

    for eth_addr in touched_addresses {
        let fil_addr = eth_addr.to_filecoin_address()?;

        // Get actor state before and after
        let pre_actor = pre_state
            .get_actor(&fil_addr)
            .map_err(|e| anyhow::anyhow!("failed to get actor state: {e}"))?;

        let post_actor = post_state
            .get_actor(&fil_addr)
            .map_err(|e| anyhow::anyhow!("failed to get actor state: {e}"))?;

        let account_diff = build_account_diff(store, pre_actor.as_ref(), post_actor.as_ref())?;

        // Only include it if there were actual changes
        state_diff.insert_if_changed(*eth_addr, account_diff);
    }

    Ok(state_diff)
}

/// Build account diff by comparing pre and post actor states.
fn build_account_diff<DB: Blockstore>(
    store: &DB,
    pre_actor: Option<&ActorState>,
    post_actor: Option<&ActorState>,
) -> anyhow::Result<AccountDiff> {
    let mut diff = AccountDiff::default();

    // Compare balance
    let pre_balance = pre_actor.map(|a| EthBigInt(a.balance.atto().clone()));
    let post_balance = post_actor.map(|a| EthBigInt(a.balance.atto().clone()));
    diff.balance = Delta::from_comparison(pre_balance, post_balance);

    // Helper to get nonce from actor (uses EVM nonce for EVM actors)
    let get_nonce = |actor: &ActorState| -> EthUint64 {
        if is_evm_actor(&actor.code) {
            EthUint64::from(
                evm::State::load(store, actor.code, actor.state)
                    .map(|s| s.nonce())
                    .unwrap_or(actor.sequence),
            )
        } else {
            EthUint64::from(actor.sequence)
        }
    };

    // Helper to get bytecode from an EVM actor
    let get_bytecode = |actor: &ActorState| -> Option<EthBytes> {
        if !is_evm_actor(&actor.code) {
            return None;
        }

        let evm_state = evm::State::load(store, actor.code, actor.state).ok()?;
        store
            .get(&evm_state.bytecode())
            .ok()
            .flatten()
            .map(EthBytes)
    };

    // Compare nonce
    let pre_nonce = pre_actor.map(get_nonce);
    let post_nonce = post_actor.map(get_nonce);
    diff.nonce = Delta::from_comparison(pre_nonce, post_nonce);

    // Compare code (bytecode for EVM actors)
    let pre_code = pre_actor.and_then(get_bytecode);
    let post_code = post_actor.and_then(get_bytecode);
    diff.code = Delta::from_comparison(pre_code, post_code);

    // Compare storage slots for EVM actors
    diff.storage = diff_evm_storage_for_actors(store, pre_actor, post_actor)?;

    Ok(diff)
}

/// Compute storage diff between pre and post actor states.
///
/// Uses different Delta types based on the scenario:
/// - Account created (None → EVM): storage slots are `Delta::Added`
/// - Account deleted (EVM → None): storage slots are `Delta::Removed`
/// - Account modified (EVM → EVM): storage slots are `Delta::Changed`
/// - Actor type changed (EVM ↔ non-EVM): treated as deletion + creation
fn diff_evm_storage_for_actors<DB: Blockstore>(
    store: &DB,
    pre_actor: Option<&ActorState>,
    post_actor: Option<&ActorState>,
) -> anyhow::Result<BTreeMap<EthHash, Delta<EthHash>>> {
    let pre_is_evm = pre_actor.is_some_and(|a| is_evm_actor(&a.code));
    let post_is_evm = post_actor.is_some_and(|a| is_evm_actor(&a.code));

    // Extract storage entries from EVM actors (empty map for non-EVM or missing actors)
    let pre_entries = extract_evm_storage_entries(store, pre_actor);
    let post_entries = extract_evm_storage_entries(store, post_actor);

    // If both are empty, no storage diff
    if pre_entries.is_empty() && post_entries.is_empty() {
        return Ok(BTreeMap::new());
    }

    let mut diff = BTreeMap::new();

    match (pre_is_evm, post_is_evm) {
        (false, true) => {
            for (key_bytes, value) in &post_entries {
                let key_hash = EthHash(ethereum_types::H256(*key_bytes));
                diff.insert(key_hash, Delta::Added(u256_to_eth_hash(value)));
            }
        }
        (true, false) => {
            for (key_bytes, value) in &pre_entries {
                let key_hash = EthHash(ethereum_types::H256(*key_bytes));
                diff.insert(key_hash, Delta::Removed(u256_to_eth_hash(value)));
            }
        }
        (true, true) => {
            for (key_bytes, pre_value) in &pre_entries {
                let key_hash = EthHash(ethereum_types::H256(*key_bytes));
                let pre_hash = u256_to_eth_hash(pre_value);

                match post_entries.get(key_bytes) {
                    Some(post_value) if pre_value != post_value => {
                        // Value changed
                        diff.insert(
                            key_hash,
                            Delta::Changed(super::types::ChangedType {
                                from: pre_hash,
                                to: u256_to_eth_hash(post_value),
                            }),
                        );
                    }
                    Some(_) => {
                        // Value unchanged, skip
                    }
                    None => {
                        // Slot cleared (value → zero)
                        diff.insert(
                            key_hash,
                            Delta::Changed(super::types::ChangedType {
                                from: pre_hash,
                                to: ZERO_HASH,
                            }),
                        );
                    }
                }
            }

            // Check for newly written entries (zero → value)
            for (key_bytes, post_value) in &post_entries {
                if !pre_entries.contains_key(key_bytes) {
                    let key_hash = EthHash(ethereum_types::H256(*key_bytes));
                    diff.insert(
                        key_hash,
                        Delta::Changed(super::types::ChangedType {
                            from: ZERO_HASH,
                            to: u256_to_eth_hash(post_value),
                        }),
                    );
                }
            }
        }
        // Neither EVM: no storage diff
        (false, false) => {}
    }

    Ok(diff)
}

/// Extract all storage entries from an EVM actor's KAMT.
/// Returns empty map if actor is None, not an EVM actor, or state cannot be loaded.
fn extract_evm_storage_entries<DB: Blockstore>(
    store: &DB,
    actor: Option<&ActorState>,
) -> HashMap<[u8; 32], U256> {
    let actor = match actor {
        Some(a) if is_evm_actor(&a.code) => a,
        _ => return HashMap::default(),
    };

    let evm_state = match evm::State::load(store, actor.code, actor.state) {
        Ok(state) => state,
        Err(e) => {
            debug!("failed to load EVM state for storage extraction: {e}");
            return HashMap::default();
        }
    };

    let storage_cid = evm_state.contract_state();
    let config = evm_kamt_config();

    let kamt: EvmStorageKamt<&DB> = match Kamt::load_with_config(&storage_cid, store, config) {
        Ok(k) => k,
        Err(e) => {
            debug!("failed to load storage KAMT: {e}");
            return HashMap::default();
        }
    };

    let mut entries = HashMap::default();
    if let Err(e) = kamt.for_each(|key, value| {
        entries.insert(key.to_big_endian(), *value);
        Ok(())
    }) {
        debug!("failed to iterate storage KAMT: {e}");
        return HashMap::default();
    }

    entries
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::MemoryDB;
    use crate::networks::ACTOR_BUNDLES_METADATA;
    use crate::rpc::eth::types::ChangedType;
    use crate::shim::address::Address as FilecoinAddress;
    use crate::shim::econ::TokenAmount;
    use crate::shim::machine::BuiltinActor;
    use crate::shim::state_tree::{StateTree, StateTreeVersion};
    use crate::utils::db::CborStoreExt as _;
    use ahash::HashSetExt as _;
    use cid::Cid;
    use num::BigInt;
    use std::sync::Arc;

    fn create_test_actor(balance_atto: u64, sequence: u64) -> ActorState {
        ActorState::new(
            Cid::default(), // Non-EVM actor code CID
            Cid::default(), // State CID (not used for non-EVM)
            TokenAmount::from_atto(balance_atto),
            sequence,
            None, // No delegated address
        )
    }

    fn get_evm_actor_code_cid() -> Option<Cid> {
        for bundle in ACTOR_BUNDLES_METADATA.values() {
            if bundle.actor_major_version().ok() == Some(17)
                && let Ok(cid) = bundle.manifest.get(BuiltinActor::EVM)
            {
                return Some(cid);
            }
        }
        None
    }

    fn create_evm_actor_with_bytecode(
        store: &MemoryDB,
        balance_atto: u64,
        actor_sequence: u64,
        evm_nonce: u64,
        bytecode: Option<&[u8]>,
    ) -> Option<ActorState> {
        use fvm_ipld_blockstore::Blockstore as _;

        let evm_code_cid = get_evm_actor_code_cid()?;

        // Store bytecode as raw bytes (not CBOR-encoded)
        let bytecode_cid = if let Some(code) = bytecode {
            use multihash_codetable::MultihashDigest;
            let mh = multihash_codetable::Code::Blake2b256.digest(code);
            let cid = Cid::new_v1(fvm_ipld_encoding::IPLD_RAW, mh);
            store.put_keyed(&cid, code).ok()?;
            cid
        } else {
            Cid::default()
        };

        let bytecode_hash = if let Some(code) = bytecode {
            use keccak_hash::keccak;
            let hash = keccak(code);
            fil_actor_evm_state::v17::BytecodeHash::from(hash.0)
        } else {
            fil_actor_evm_state::v17::BytecodeHash::EMPTY
        };

        let evm_state = fil_actor_evm_state::v17::State {
            bytecode: bytecode_cid,
            bytecode_hash,
            contract_state: Cid::default(),
            transient_data: None,
            nonce: evm_nonce,
            tombstone: None,
        };

        let state_cid = store.put_cbor_default(&evm_state).ok()?;

        Some(ActorState::new(
            evm_code_cid,
            state_cid,
            TokenAmount::from_atto(balance_atto),
            actor_sequence,
            None,
        ))
    }

    fn create_masked_id_eth_address(actor_id: u64) -> EthAddress {
        EthAddress::from_actor_id(actor_id)
    }

    struct TestStateTrees {
        store: Arc<MemoryDB>,
        pre_state: StateTree<MemoryDB>,
        post_state: StateTree<MemoryDB>,
    }

    impl TestStateTrees {
        fn new() -> anyhow::Result<Self> {
            let store = Arc::new(MemoryDB::default());
            // Use V4 which creates FvmV2 state trees that allow direct set_actor
            let pre_state = StateTree::new(store.clone(), StateTreeVersion::V5)?;
            let post_state = StateTree::new(store.clone(), StateTreeVersion::V5)?;
            Ok(Self {
                store,
                pre_state,
                post_state,
            })
        }

        /// Create state trees with different actors in pre and post.
        fn with_changed_actor(
            actor_id: u64,
            pre_actor: ActorState,
            post_actor: ActorState,
        ) -> anyhow::Result<Self> {
            let store = Arc::new(MemoryDB::default());
            let mut pre_state = StateTree::new(store.clone(), StateTreeVersion::V5)?;
            let mut post_state = StateTree::new(store.clone(), StateTreeVersion::V5)?;
            let addr = FilecoinAddress::new_id(actor_id);
            pre_state.set_actor(&addr, pre_actor)?;
            post_state.set_actor(&addr, post_actor)?;
            Ok(Self {
                store,
                pre_state,
                post_state,
            })
        }

        /// Create state trees with actor only in post (creation scenario).
        fn with_created_actor(actor_id: u64, post_actor: ActorState) -> anyhow::Result<Self> {
            let store = Arc::new(MemoryDB::default());
            let pre_state = StateTree::new(store.clone(), StateTreeVersion::V5)?;
            let mut post_state = StateTree::new(store.clone(), StateTreeVersion::V5)?;
            let addr = FilecoinAddress::new_id(actor_id);
            post_state.set_actor(&addr, post_actor)?;
            Ok(Self {
                store,
                pre_state,
                post_state,
            })
        }

        /// Create state trees with actor only in pre (deletion scenario).
        fn with_deleted_actor(actor_id: u64, pre_actor: ActorState) -> anyhow::Result<Self> {
            let store = Arc::new(MemoryDB::default());
            let mut pre_state = StateTree::new(store.clone(), StateTreeVersion::V5)?;
            let post_state = StateTree::new(store.clone(), StateTreeVersion::V5)?;
            let addr = FilecoinAddress::new_id(actor_id);
            pre_state.set_actor(&addr, pre_actor)?;
            Ok(Self {
                store,
                pre_state,
                post_state,
            })
        }

        /// Build state diff for given touched addresses.
        fn build_diff(&self, touched_addresses: &HashSet<EthAddress>) -> anyhow::Result<StateDiff> {
            build_state_diff(
                self.store.as_ref(),
                &self.pre_state,
                &self.post_state,
                touched_addresses,
            )
        }
    }

    #[test]
    fn test_build_state_diff_empty_touched_addresses() {
        let trees = TestStateTrees::new().unwrap();
        let touched_addresses = HashSet::new();

        let state_diff = trees.build_diff(&touched_addresses).unwrap();

        // No addresses touched = empty state diff
        assert!(state_diff.0.is_empty());
    }

    #[test]
    fn test_build_state_diff_nonexistent_address() {
        let trees = TestStateTrees::new().unwrap();
        let mut touched_addresses = HashSet::new();
        touched_addresses.insert(create_masked_id_eth_address(9999));

        let state_diff = trees.build_diff(&touched_addresses).unwrap();

        // Address doesn't exist in either state, so no diff (both None = unchanged)
        assert!(state_diff.0.is_empty());
    }

    #[test]
    fn test_build_state_diff_balance_increase() {
        let actor_id = 1001u64;
        let pre_actor = create_test_actor(1000, 5);
        let post_actor = create_test_actor(2000, 5);
        let trees = TestStateTrees::with_changed_actor(actor_id, pre_actor, post_actor).unwrap();

        let mut touched_addresses = HashSet::new();
        touched_addresses.insert(create_masked_id_eth_address(actor_id));

        let state_diff = trees.build_diff(&touched_addresses).unwrap();

        assert_eq!(state_diff.0.len(), 1);
        let eth_addr = create_masked_id_eth_address(actor_id);
        let diff = state_diff.0.get(&eth_addr).unwrap();
        match &diff.balance {
            Delta::Changed(change) => {
                assert_eq!(change.from.0, BigInt::from(1000));
                assert_eq!(change.to.0, BigInt::from(2000));
            }
            _ => panic!("Expected Delta::Changed for balance"),
        }
        assert!(diff.nonce.is_unchanged());
    }

    #[test]
    fn test_build_state_diff_balance_decrease() {
        let actor_id = 1002u64;
        let pre_actor = create_test_actor(5000, 10);
        let post_actor = create_test_actor(3000, 10);
        let trees = TestStateTrees::with_changed_actor(actor_id, pre_actor, post_actor).unwrap();

        let mut touched_addresses = HashSet::new();
        touched_addresses.insert(create_masked_id_eth_address(actor_id));

        let state_diff = trees.build_diff(&touched_addresses).unwrap();

        let eth_addr = create_masked_id_eth_address(actor_id);
        let diff = state_diff.0.get(&eth_addr).unwrap();
        match &diff.balance {
            Delta::Changed(change) => {
                assert_eq!(change.from.0, BigInt::from(5000));
                assert_eq!(change.to.0, BigInt::from(3000));
            }
            _ => panic!("Expected Delta::Changed for balance"),
        }
        assert!(diff.nonce.is_unchanged());
    }

    #[test]
    fn test_build_state_diff_nonce_increment() {
        let actor_id = 1003u64;
        let pre_actor = create_test_actor(1000, 5);
        let post_actor = create_test_actor(1000, 6);
        let trees = TestStateTrees::with_changed_actor(actor_id, pre_actor, post_actor).unwrap();

        let mut touched_addresses = HashSet::new();
        touched_addresses.insert(create_masked_id_eth_address(actor_id));

        let state_diff = trees.build_diff(&touched_addresses).unwrap();

        let eth_addr = create_masked_id_eth_address(actor_id);
        let diff = state_diff.0.get(&eth_addr).unwrap();
        assert!(diff.balance.is_unchanged());
        match &diff.nonce {
            Delta::Changed(change) => {
                assert_eq!(change.from.0, 5);
                assert_eq!(change.to.0, 6);
            }
            _ => panic!("Expected Delta::Changed for nonce"),
        }
    }

    #[test]
    fn test_build_state_diff_both_balance_and_nonce_change() {
        let actor_id = 1004u64;
        let pre_actor = create_test_actor(10000, 100);
        let post_actor = create_test_actor(9000, 101);
        let trees = TestStateTrees::with_changed_actor(actor_id, pre_actor, post_actor).unwrap();

        let mut touched_addresses = HashSet::new();
        touched_addresses.insert(create_masked_id_eth_address(actor_id));

        let state_diff = trees.build_diff(&touched_addresses).unwrap();

        let eth_addr = create_masked_id_eth_address(actor_id);
        let diff = state_diff.0.get(&eth_addr).unwrap();
        match &diff.balance {
            Delta::Changed(change) => {
                assert_eq!(change.from.0, BigInt::from(10000));
                assert_eq!(change.to.0, BigInt::from(9000));
            }
            _ => panic!("Expected Delta::Changed for balance"),
        }
        match &diff.nonce {
            Delta::Changed(change) => {
                assert_eq!(change.from.0, 100);
                assert_eq!(change.to.0, 101);
            }
            _ => panic!("Expected Delta::Changed for nonce"),
        }
    }

    #[test]
    fn test_build_state_diff_account_creation() {
        let actor_id = 1005u64;
        let post_actor = create_test_actor(5000, 0);
        let trees = TestStateTrees::with_created_actor(actor_id, post_actor).unwrap();

        let mut touched_addresses = HashSet::new();
        touched_addresses.insert(create_masked_id_eth_address(actor_id));

        let state_diff = trees.build_diff(&touched_addresses).unwrap();

        let eth_addr = create_masked_id_eth_address(actor_id);
        let diff = state_diff.0.get(&eth_addr).unwrap();
        match &diff.balance {
            Delta::Added(balance) => {
                assert_eq!(balance.0, BigInt::from(5000));
            }
            _ => panic!("Expected Delta::Added for balance"),
        }
        match &diff.nonce {
            Delta::Added(nonce) => {
                assert_eq!(nonce.0, 0);
            }
            _ => panic!("Expected Delta::Added for nonce"),
        }
    }

    #[test]
    fn test_build_state_diff_account_deletion() {
        let actor_id = 1006u64;
        let pre_actor = create_test_actor(3000, 10);
        let trees = TestStateTrees::with_deleted_actor(actor_id, pre_actor).unwrap();

        let mut touched_addresses = HashSet::new();
        touched_addresses.insert(create_masked_id_eth_address(actor_id));

        let state_diff = trees.build_diff(&touched_addresses).unwrap();

        let eth_addr = create_masked_id_eth_address(actor_id);
        let diff = state_diff.0.get(&eth_addr).unwrap();
        match &diff.balance {
            Delta::Removed(balance) => {
                assert_eq!(balance.0, BigInt::from(3000));
            }
            _ => panic!("Expected Delta::Removed for balance"),
        }
        match &diff.nonce {
            Delta::Removed(nonce) => {
                assert_eq!(nonce.0, 10);
            }
            _ => panic!("Expected Delta::Removed for nonce"),
        }
    }

    #[test]
    fn test_build_state_diff_multiple_addresses() {
        let store = Arc::new(MemoryDB::default());
        let mut pre_state = StateTree::new(store.clone(), StateTreeVersion::V5).unwrap();
        let mut post_state = StateTree::new(store.clone(), StateTreeVersion::V5).unwrap();

        // Actor 1: balance increase
        let addr1 = FilecoinAddress::new_id(2001);
        pre_state
            .set_actor(&addr1, create_test_actor(1000, 0))
            .unwrap();
        post_state
            .set_actor(&addr1, create_test_actor(2000, 0))
            .unwrap();

        // Actor 2: nonce increase
        let addr2 = FilecoinAddress::new_id(2002);
        pre_state
            .set_actor(&addr2, create_test_actor(500, 5))
            .unwrap();
        post_state
            .set_actor(&addr2, create_test_actor(500, 6))
            .unwrap();

        // Actor 3: no change (should not appear in diff)
        let addr3 = FilecoinAddress::new_id(2003);
        pre_state
            .set_actor(&addr3, create_test_actor(100, 1))
            .unwrap();
        post_state
            .set_actor(&addr3, create_test_actor(100, 1))
            .unwrap();

        let mut touched_addresses = HashSet::new();
        touched_addresses.insert(create_masked_id_eth_address(2001));
        touched_addresses.insert(create_masked_id_eth_address(2002));
        touched_addresses.insert(create_masked_id_eth_address(2003));

        let state_diff =
            build_state_diff(store.as_ref(), &pre_state, &post_state, &touched_addresses).unwrap();

        assert_eq!(state_diff.0.len(), 2);
        assert!(
            state_diff
                .0
                .contains_key(&create_masked_id_eth_address(2001))
        );
        assert!(
            state_diff
                .0
                .contains_key(&create_masked_id_eth_address(2002))
        );
        assert!(
            !state_diff
                .0
                .contains_key(&create_masked_id_eth_address(2003))
        );
    }

    #[test]
    fn test_build_state_diff_evm_actor_scenarios() {
        struct TestCase {
            name: &'static str,
            pre: Option<(u64, u64, Option<&'static [u8]>)>, // balance, nonce, bytecode
            post: Option<(u64, u64, Option<&'static [u8]>)>,
            expected_balance: Delta<EthBigInt>,
            expected_nonce: Delta<EthUint64>,
            expected_code: Delta<EthBytes>,
        }

        let bytecode1: &[u8] = &[0x60, 0x80, 0x60, 0x40, 0x52];
        let bytecode2: &[u8] = &[0x60, 0x80, 0x60, 0x40, 0x52, 0x00];

        let cases = vec![
            TestCase {
                name: "No change",
                pre: Some((1000, 5, Some(bytecode1))),
                post: Some((1000, 5, Some(bytecode1))),
                expected_balance: Delta::Unchanged,
                expected_nonce: Delta::Unchanged,
                expected_code: Delta::Unchanged,
            },
            TestCase {
                name: "Balance increase",
                pre: Some((1000, 5, Some(bytecode1))),
                post: Some((2000, 5, Some(bytecode1))),
                expected_balance: Delta::Changed(ChangedType {
                    from: EthBigInt(BigInt::from(1000)),
                    to: EthBigInt(BigInt::from(2000)),
                }),
                expected_nonce: Delta::Unchanged,
                expected_code: Delta::Unchanged,
            },
            TestCase {
                name: "Nonce increment",
                pre: Some((1000, 5, Some(bytecode1))),
                post: Some((1000, 6, Some(bytecode1))),
                expected_balance: Delta::Unchanged,
                expected_nonce: Delta::Changed(ChangedType {
                    from: EthUint64(5),
                    to: EthUint64(6),
                }),
                expected_code: Delta::Unchanged,
            },
            TestCase {
                name: "Bytecode change",
                pre: Some((1000, 5, Some(bytecode1))),
                post: Some((1000, 5, Some(bytecode2))),
                expected_balance: Delta::Unchanged,
                expected_nonce: Delta::Unchanged,
                expected_code: Delta::Changed(ChangedType {
                    from: EthBytes(bytecode1.to_vec()),
                    to: EthBytes(bytecode2.to_vec()),
                }),
            },
            TestCase {
                name: "Balance and Nonce change",
                pre: Some((1000, 5, Some(bytecode1))),
                post: Some((2000, 6, Some(bytecode1))),
                expected_balance: Delta::Changed(ChangedType {
                    from: EthBigInt(BigInt::from(1000)),
                    to: EthBigInt(BigInt::from(2000)),
                }),
                expected_nonce: Delta::Changed(ChangedType {
                    from: EthUint64(5),
                    to: EthUint64(6),
                }),
                expected_code: Delta::Unchanged,
            },
            TestCase {
                name: "Creation",
                pre: None,
                post: Some((5000, 0, Some(bytecode1))),
                expected_balance: Delta::Added(EthBigInt(BigInt::from(5000))),
                expected_nonce: Delta::Added(EthUint64(0)),
                expected_code: Delta::Added(EthBytes(bytecode1.to_vec())),
            },
            TestCase {
                name: "Deletion",
                pre: Some((3000, 10, Some(bytecode1))),
                post: None,
                expected_balance: Delta::Removed(EthBigInt(BigInt::from(3000))),
                expected_nonce: Delta::Removed(EthUint64(10)),
                expected_code: Delta::Removed(EthBytes(bytecode1.to_vec())),
            },
        ];

        for case in cases {
            let store = Arc::new(MemoryDB::default());
            let actor_id = 10000u64; // arbitrary ID

            let pre_actor = case.pre.and_then(|(bal, nonce, code)| {
                create_evm_actor_with_bytecode(&store, bal, 0, nonce, code)
            });
            let post_actor = case.post.and_then(|(bal, nonce, code)| {
                create_evm_actor_with_bytecode(&store, bal, 0, nonce, code)
            });

            let mut pre_state = StateTree::new(store.clone(), StateTreeVersion::V5).unwrap();
            let mut post_state = StateTree::new(store.clone(), StateTreeVersion::V5).unwrap();
            let addr = FilecoinAddress::new_id(actor_id);

            if let Some(actor) = pre_actor {
                pre_state.set_actor(&addr, actor).unwrap();
            }
            if let Some(actor) = post_actor {
                post_state.set_actor(&addr, actor).unwrap();
            }

            let mut touched_addresses = HashSet::new();
            touched_addresses.insert(create_masked_id_eth_address(actor_id));

            let state_diff =
                build_state_diff(store.as_ref(), &pre_state, &post_state, &touched_addresses)
                    .unwrap();

            if case.expected_balance == Delta::Unchanged
                && case.expected_nonce == Delta::Unchanged
                && case.expected_code == Delta::Unchanged
            {
                assert!(
                    state_diff.0.is_empty(),
                    "Test case '{}' failed: expected empty diff",
                    case.name
                );
            } else {
                let eth_addr = create_masked_id_eth_address(actor_id);
                let diff = state_diff.0.get(&eth_addr).unwrap_or_else(|| {
                    panic!("Test case '{}' failed: missing diff entry", case.name)
                });

                assert_eq!(
                    diff.balance, case.expected_balance,
                    "Test case '{}' failed: balance mismatch",
                    case.name
                );
                assert_eq!(
                    diff.nonce, case.expected_nonce,
                    "Test case '{}' failed: nonce mismatch",
                    case.name
                );
                assert_eq!(
                    diff.code, case.expected_code,
                    "Test case '{}' failed: code mismatch",
                    case.name
                );
            }
        }
    }

    #[test]
    fn test_build_state_diff_non_evm_actor_no_code() {
        // Non-EVM actors should have no code in their diff
        let actor_id = 4005u64;
        let pre_actor = create_test_actor(1000, 5);
        let post_actor = create_test_actor(2000, 6);
        let trees = TestStateTrees::with_changed_actor(actor_id, pre_actor, post_actor).unwrap();

        let mut touched_addresses = HashSet::new();
        touched_addresses.insert(create_masked_id_eth_address(actor_id));

        let state_diff = trees.build_diff(&touched_addresses).unwrap();

        let eth_addr = create_masked_id_eth_address(actor_id);
        let diff = state_diff.0.get(&eth_addr).unwrap();

        // Balance and nonce should change
        assert!(!diff.balance.is_unchanged());
        assert!(!diff.nonce.is_unchanged());

        // Code should be unchanged (None -> None for non-EVM actors)
        assert!(diff.code.is_unchanged());
    }
}
