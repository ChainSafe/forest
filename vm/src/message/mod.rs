mod errors;
mod message_receipt;
mod signature;
mod signed_message;
mod signer;

pub use errors::*;
pub use message_receipt::*;
pub use signature::*;
pub use signed_message::*;
pub use signer::*;

use crate::address::Address;
use num_bigint::BigUint;

/// VM message type which includes all data needed for a state transition
#[derive(PartialEq, Clone)]
pub struct Message {
    pub(crate) from: Address,
    pub(crate) to: Address,

    pub(crate) nonce: u64,

    pub(crate) value: BigUint,

    pub(crate) method: u64,
    pub(crate) params: Vec<u8>,

    pub(crate) gas_price: BigUint, // change these to big int
    pub(crate) gas_limit: BigUint,
}

impl Message {
    // Marshalling and unmarshalling
    pub fn unmarshall_cbor(&mut self, _bz: &mut [u8]) -> Result<(), String> {
        // TODO
        Err("Unmarshall cbor not implemented".to_owned())
    }
    pub fn marshall_cbor(&self) -> Result<Vec<u8>, String> {
        // TODO
        Err("Marshall cbor not implemented".to_owned())
    }
}
