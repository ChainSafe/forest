use num_bigint::BigUint;

/// MessageReceipt is the result of a state transition from a message
#[derive(PartialEq, Clone)]
pub struct MessageReceipt {
    // TODO: determine if this is necessary, code returned from cbor
    pub(crate) exit_code: u8,
    pub(crate) return_data: Vec<u8>,
    pub(crate) gas_used: BigUint,
}
