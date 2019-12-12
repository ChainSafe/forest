use num_bigint::BigUint;
use vm::ExitCode;

/// MessageReceipt is the result of a state transition from a message
#[derive(PartialEq, Clone)]
pub struct MessageReceipt {
    // TODO: determine if this is necessary, code returned from cbor
    pub exit_code: ExitCode,
    pub return_data: Vec<u8>,
    pub gas_used: BigUint,
}
