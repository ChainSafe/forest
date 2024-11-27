use super::types::{EthAddress, EthBlockTrace, EthBytes};
use crate::rpc::methods::eth::lookup_eth_address;
use crate::rpc::methods::state::{ExecutionTrace, MessageTrace, ReturnTrace};
use crate::rpc::state::ActorTrace;
use crate::shim::{address::Address, clock::ChainEpoch, error::ExitCode, state_tree::StateTree};
use anyhow::{bail, Context};
use cid::Cid;
use fil_actor_evm_state::v15 as code;
use fvm_ipld_blockstore::Blockstore;
use fvm_shared3::error::ExitCode as ExitCodeV3;

pub fn decode_payload(payload: &[u8], codec: u64) -> anyhow::Result<EthBytes> {
    todo!()
}

pub fn decode_params() -> anyhow::Result<MessageTrace> {
    todo!()
}

pub fn decode_return() -> anyhow::Result<ReturnTrace> {
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
    if code == ExitCodeV3::SYS_OUT_OF_GAS.into() {
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
) -> anyhow::Result<EthBlockTrace> {
    todo!()
}

// Build an EthTrace for a "call", parsing the inputs & outputs as a "native" FVM call.
pub fn trace_native_call(
    env: &mut Environment,
    address: &[i64],
    trace: &ExecutionTrace,
) -> anyhow::Result<EthBlockTrace> {
    todo!()
}

// Build an EthTrace for a "call", parsing the inputs & outputs as an EVM call (falling back on
// treating it as a native call).
pub fn trace_evm_call(
    env: &mut Environment,
    address: &[i64],
    trace: &ExecutionTrace,
) -> anyhow::Result<(EthBlockTrace, ExecutionTrace)> {
    todo!()
}

// Build an EthTrace for a native "create" operation. This should only be called with an
// ExecutionTrace is an Exec or Exec4 method invocation on the Init actor.
pub fn trace_native_create(
    env: &mut Environment,
    address: &[i64],
    trace: &ExecutionTrace,
) -> anyhow::Result<(EthBlockTrace, ExecutionTrace)> {
    todo!()
}

// Decode the parameters and return value of an EVM smart contract creation through the EAM. This
// should only be called with an ExecutionTrace for a Create, Create2, or CreateExternal method
// invocation on the EAM.
pub fn decode_create_via_eam(trace: &ExecutionTrace) -> anyhow::Result<(Vec<u8>, EthAddress)> {
    todo!()
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
