mod message_receipt;
mod signed_message;

pub use message_receipt::*;
pub use signed_message::*;

use address::Address;
use encoding::{Cbor, CodecProtocol, Error as EncodingError};
use num_bigint::BigUint;

/// VM message type which includes all data needed for a state transition
#[derive(PartialEq, Clone)]
pub struct Message {
    from: Address,
    to: Address,

    pub(crate) sequence: u64,

    pub(crate) value: BigUint,

    pub(crate) method_id: u64,
    pub(crate) params: Vec<u8>,

    pub(crate) gas_price: BigUint,
    pub(crate) gas_limit: BigUint,
}

impl Message {
    /// from returns the from address of the message
    pub fn from(&self) -> Address {
        self.from.clone()
    }
    /// to returns the destination address of the message
    pub fn to(&self) -> Address {
        self.to.clone()
    }
}

impl Cbor for Message {
    fn unmarshall_cbor(&mut self, _bz: &mut [u8]) -> Result<(), EncodingError> {
        // TODO
        Err(EncodingError::Unmarshalling(CodecProtocol::Cbor))
    }
    fn marshall_cbor(&self) -> Result<Vec<u8>, EncodingError> {
        // TODO
        Err(EncodingError::Marshalling(CodecProtocol::Cbor))
    }
}
