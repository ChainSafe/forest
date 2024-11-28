use super::types::{
    EthAddress, EthBlockTrace, EthBytes, EthCallTraceAction, TraceAction, TraceResult,
};
use super::{
    decode_payload, decode_return, encode_filecoin_params_as_abi, encode_filecoin_returns_as_abi,
    EthCallTraceResult, EthCreateTraceAction, EthCreateTraceResult,
};
use crate::eth::EAMMethod;
use crate::rpc::methods::eth::lookup_eth_address;
use crate::rpc::methods::state::{ExecutionTrace, MessageTrace};
use crate::rpc::state::ActorTrace;
use crate::shim::{address::Address, error::ExitCode, state_tree::StateTree};
use anyhow::{bail, Context};
use fil_actor_eam_state::v12 as eam12;
use fil_actor_evm_state::v15 as code;
use fil_actor_init_state::v12::ExecReturn;
use fil_actor_init_state::v15::Method as InitMethod;
use fvm_ipld_blockstore::Blockstore;
use fvm_shared4::error::ExitCode as ExitCodeV4;
use fvm_shared4::METHOD_CONSTRUCTOR;

pub fn decode_params() -> anyhow::Result<MessageTrace> {
    todo!()
}

#[derive(Default)]
pub struct Environment {
    caller: EthAddress,
    is_evm: bool,
    subtrace_count: i64,
    pub traces: Vec<EthBlockTrace>,
    last_byte_code: EthAddress,
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

pub fn trace_to_address(trace: &ActorTrace) -> EthAddress {
    if let Some(addr) = trace.state.delegated_address {
        if let Ok(eth_addr) = EthAddress::from_filecoin_address(&addr.into()) {
            return eth_addr;
        }
    }
    EthAddress::from_actor_id(trace.id)
}

/// Returns true if the trace is a call to an EVM or EAM actor.
pub fn trace_is_evm_or_eam(trace: &ExecutionTrace) -> bool {
    true
}

/// Returns true if the trace is a call to an EVM or EAM actor.
pub fn trace_err_msg(trace: &ExecutionTrace) -> String {
    let code = trace.msg_rct.exit_code;

    if code.is_success() {
        return "".into();
    }

    // EVM tools often expect this literal string.
    if code == ExitCodeV4::SYS_OUT_OF_GAS.into() {
        return "out of gas".into();
    }

    // indicate when we have a "system" error.
    if code.value() < ExitCode::FIRST_ACTOR_ERROR_CODE {
        return format!("vm error: {}", code.value());
    }

    // handle special exit codes from the EVM/EAM.
    if trace_is_evm_or_eam(trace) {
        match code.into() {
            code::EVM_CONTRACT_REVERTED => return "Reverted".into(), // capitalized for compatibility
            code::EVM_CONTRACT_INVALID_INSTRUCTION => return "invalid instruction".into(),
            code::EVM_CONTRACT_UNDEFINED_INSTRUCTION => return "undefined instruction".into(),
            code::EVM_CONTRACT_STACK_UNDERFLOW => return "stack underflow".into(),
            code::EVM_CONTRACT_STACK_OVERFLOW => return "stack overflow".into(),
            code::EVM_CONTRACT_ILLEGAL_MEMORY_ACCESS => return "illegal memory access".into(),
            code::EVM_CONTRACT_BAD_JUMPDEST => return "invalid jump destination".into(),
            code::EVM_CONTRACT_SELFDESTRUCT_FAILED => return "self destruct failed".into(),
            _ => (),
        }
    }
    // everything else...
    format!("actor error: {}", code.value())
}

/// Recursively builds the traces for a given ExecutionTrace by walking the subcalls
pub fn build_traces(
    env: &mut Environment,
    address: &[i64],
    trace: &ExecutionTrace,
) -> anyhow::Result<()> {
    todo!()
}

// buildTrace processes the passed execution trace and updates the environment, if necessary.
//
// On success, it returns a trace to add (or nil to skip) and the trace recurse into (or nil to skip).
pub fn build_trace(
    env: &mut Environment,
    address: &[i64],
    trace: &Option<ExecutionTrace>,
) -> anyhow::Result<ExecutionTrace> {
    todo!()
}

// Build an EthTrace for a "call" with the given input & output.
pub fn trace_call(
    env: &mut Environment,
    address: &[i64],
    trace: &ExecutionTrace,
    input: EthBytes,
    output: EthBytes,
) -> anyhow::Result<EthBlockTrace> {
    if let Some(invoked_actor) = &trace.invoked_actor {
        let to = trace_to_address(invoked_actor);
        let call_type: String = if trace.msg.read_only.unwrap_or_default() {
            "staticcall"
        } else {
            "call"
        }
        .into();

        let default = EthBlockTrace::default();
        Ok(EthBlockTrace {
            r#type: "call".into(),
            action: TraceAction::Call(EthCallTraceAction {
                call_type,
                from: env.caller.clone(),
                to,
                gas: trace.msg.gas_limit.unwrap_or_default().into(),
                value: trace.msg.value.clone().into(),
                input,
            }),
            result: TraceResult::Call(EthCallTraceResult {
                gas_used: 0.into(),
                output,
            }),
            trace_address: Vec::from(address),
            error: trace_err_msg(trace),
            ..default
        })
    } else {
        bail!("no invoked actor")
    }
}

// Build an EthTrace for a "call", parsing the inputs & outputs as a "native" FVM call.
pub fn trace_native_call(
    env: &mut Environment,
    address: &[i64],
    trace: &ExecutionTrace,
) -> anyhow::Result<EthBlockTrace> {
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
pub fn trace_evm_call(
    env: &mut Environment,
    address: &[i64],
    trace: ExecutionTrace,
) -> anyhow::Result<(EthBlockTrace, ExecutionTrace)> {
    let input = decode_payload(&trace.msg.params, trace.msg.params_codec)?;
    let output = decode_payload(&trace.msg_rct.r#return, trace.msg_rct.return_codec)?;
    // TODO(elmattic): add debug logs

    Ok((trace_call(env, address, &trace, input, output)?, trace))
}

// Build an EthTrace for a native "create" operation. This should only be called with an
// ExecutionTrace is an Exec or Exec4 method invocation on the Init actor.
pub fn trace_native_create(
    env: &mut Environment,
    address: &[i64],
    trace: ExecutionTrace,
) -> anyhow::Result<(Option<EthBlockTrace>, Option<ExecutionTrace>)> {
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
        .find(|c| c.msg.method == (METHOD_CONSTRUCTOR as u64));

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
        Some(EthBlockTrace {
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
                gas_used: 0.into(),
                address: Some(create_addr),
                code: output,
            }),
            trace_address: Vec::from(address),
            error: trace_err_msg(&trace),
            ..EthBlockTrace::default()
        }),
        Some(sub_trace.clone()),
    ))
}

// Decode the parameters and return value of an EVM smart contract creation through the EAM. This
// should only be called with an ExecutionTrace for a Create, Create2, or CreateExternal method
// invocation on the EAM.
pub fn decode_create_via_eam(trace: &ExecutionTrace) -> anyhow::Result<(Vec<u8>, EthAddress)> {
    let method: EAMMethod = EAMMethod::from_repr(trace.msg.method)
        .with_context(|| format!("unexpected CREATE method {}", trace.msg.method))?;

    let init_code = match method {
        EAMMethod::Create => {
            todo!()
        }
        EAMMethod::Create2 => {
            todo!()
        }
        EAMMethod::CreateExternal => {
            todo!()
        }
        _ => bail!("unexpected CREATE method {}", trace.msg.method),
    };
    // let ret = decode_return(&trace.msg_rct)?;

    Ok((init_code, todo!()))
}

// Build an EthTrace for an EVM "create" operation. This should only be called with an
// ExecutionTrace for a Create, Create2, or CreateExternal method invocation on the EAM.
pub fn trace_eth_create(
    env: &mut Environment,
    address: &[i64],
    trace: &ExecutionTrace,
) -> anyhow::Result<(EthBlockTrace, ExecutionTrace)> {
    todo!()
}

// Build an EthTrace for a "private" method invocation from the EVM. This should only be called with
// an ExecutionTrace from an EVM instance and on a method between 1 and 1023 inclusive.
pub fn trace_evm_private(
    env: &mut Environment,
    address: &[i64],
    trace: &ExecutionTrace,
) -> anyhow::Result<(EthBlockTrace, ExecutionTrace)> {
    todo!()
}
