// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::types::{
    EthAddress, EthBytes, EthCallTraceAction, EthTrace, TraceAction, TraceResult,
};
use super::utils::{decode_params, decode_return};
use super::{
    decode_payload, encode_filecoin_params_as_abi, encode_filecoin_returns_as_abi,
    EthCallTraceResult, EthCreateTraceAction, EthCreateTraceResult,
};
use crate::eth::{EAMMethod, EVMMethod};
use crate::rpc::methods::eth::lookup_eth_address;
use crate::rpc::methods::state::ExecutionTrace;
use crate::rpc::state::ActorTrace;
use crate::shim::fvm_shared_latest::METHOD_CONSTRUCTOR;
use crate::shim::{actors::is_evm_actor, address::Address, error::ExitCode, state_tree::StateTree};
use fil_actor_eam_state::v12 as eam12;
use fil_actor_evm_state::v15 as evm12;
use fil_actor_init_state::v12::ExecReturn;
use fil_actor_init_state::v15::Method as InitMethod;
use fvm_ipld_blockstore::Blockstore;

use anyhow::{bail, Context};
use num::FromPrimitive;
use tracing::debug;

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
        .with_context(|| format!("top-level message sender {} s could not be found", from))?;
    Ok(Environment {
        caller: sender,
        ..Environment::default()
    })
}

fn trace_to_address(trace: &ActorTrace) -> EthAddress {
    if let Some(addr) = trace.state.delegated_address {
        if let Ok(eth_addr) = EthAddress::from_filecoin_address(&addr.into()) {
            return eth_addr;
        }
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
        return Some(format!("vm error: {}", code));
    }

    // handle special exit codes from the EVM/EAM.
    if trace_is_evm_or_eam(trace) {
        match code.into() {
            evm12::EVM_CONTRACT_REVERTED => return Some("Reverted".into()), // capitalized for compatibility
            evm12::EVM_CONTRACT_INVALID_INSTRUCTION => return Some("invalid instruction".into()),
            evm12::EVM_CONTRACT_UNDEFINED_INSTRUCTION => {
                return Some("undefined instruction".into())
            }
            evm12::EVM_CONTRACT_STACK_UNDERFLOW => return Some("stack underflow".into()),
            evm12::EVM_CONTRACT_STACK_OVERFLOW => return Some("stack overflow".into()),
            evm12::EVM_CONTRACT_ILLEGAL_MEMORY_ACCESS => {
                return Some("illegal memory access".into())
            }
            evm12::EVM_CONTRACT_BAD_JUMPDEST => return Some("invalid jump destination".into()),
            evm12::EVM_CONTRACT_SELFDESTRUCT_FAILED => return Some("self destruct failed".into()),
            _ => (),
        }
    }
    // everything else...
    Some(format!("actor error: {}", code))
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
                from: env.caller.clone(),
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
                from: env.caller.clone(),
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
                from: env.caller.clone(),
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
            if trace.msg_rct.exit_code.is_success() {
                if let Option::Some(actor_trace) = &trace.invoked_actor {
                    let to = trace_to_address(actor_trace);
                    env.last_byte_code = Some(to);
                }
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
                        from: env.caller.clone(),
                        to: env.last_byte_code.clone(),
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
