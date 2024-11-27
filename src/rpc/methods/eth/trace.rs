use super::types::{EthAddress, EthBlockTrace, EthBytes};
use crate::rpc::methods::state::{ExecutionTrace, MessageTrace, ReturnTrace};
use crate::rpc::state::ActorTrace;
use crate::shim::{clock::ChainEpoch, state_tree::StateTree};
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;

pub fn decode_payload(payload: &[u8], codec: u64) -> anyhow::Result<EthBytes> {
    todo!()
}

pub fn decode_params() -> anyhow::Result<MessageTrace> {
    todo!()
}

pub fn decode_return() -> anyhow::Result<ReturnTrace> {
    todo!()
}

pub struct Environment {
    caller: EthAddress,
    is_evm: bool,
    subtrace_count: i64,
    traces: Vec<EthBlockTrace>,
    last_byte_code: EthAddress,
}

pub fn base_environment<BS: Blockstore + Send + Sync>(
    state: StateTree<BS>,
    from: &EthAddress,
) -> anyhow::Result<Environment> {
    todo!()
}

pub fn trace_to_address(trace: &ActorTrace) -> EthAddress {
    todo!()
}

/// Returns true if the trace is a call to an EVM or EAM actor.
pub fn trace_is_evm_or_eam(trace: &ExecutionTrace) -> bool {
    todo!()
}

/// Returns true if the trace is a call to an EVM or EAM actor.
pub fn trace_err_msg(trace: &ExecutionTrace) -> String {
    todo!()
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
    trace: &ExecutionTrace,
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
