use super::Message;

use address::Address;
use encoding::{Cbor, CodecProtocol, Error as EncodingError};
use num_bigint::BigUint;

/// VM message type which includes all data needed for a state transition
#[derive(PartialEq, Clone, Debug)]
pub struct UnsignedMessage {
    from: Address,
    to: Address,
    sequence: u64,
    value: BigUint,
    method_num: u64,
    params: Vec<u8>,
    gas_price: BigUint,
    gas_limit: BigUint,
}

impl UnsignedMessage {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        from: Address,
        to: Address,
        sequence: u64,
        value: BigUint,
        method_num: u64,
        params: Vec<u8>,
        gas_price: BigUint,
        gas_limit: BigUint,
    ) -> Self {
        Self {
            from,
            to,
            sequence,
            value,
            method_num,
            params,
            gas_price,
            gas_limit,
        }
    }
}

impl Message for UnsignedMessage {
    /// from returns the from address of the message
    fn from(&self) -> Address {
        self.from.clone()
    }
    /// to returns the destination address of the message
    fn to(&self) -> Address {
        self.to.clone()
    }
    /// sequence returns the message sequence or nonce
    fn sequence(&self) -> u64 {
        self.sequence
    }
    /// value returns the amount sent in message
    fn value(&self) -> BigUint {
        self.value.clone()
    }
    /// method_num returns the method number to be called
    fn method_num(&self) -> u64 {
        self.method_num
    }
    /// params returns the encoded parameters for the method call
    fn params(&self) -> Vec<u8> {
        self.params.clone()
    }
    /// gas_price returns gas price for the message
    fn gas_price(&self) -> BigUint {
        self.gas_price.clone()
    }
    /// gas_limit returns the gas limit for the message
    fn gas_limit(&self) -> BigUint {
        self.gas_limit.clone()
    }
}

impl Cbor for UnsignedMessage {
    fn unmarshal_cbor(_bz: &[u8]) -> Result<Self, EncodingError> {
        // TODO
        Err(EncodingError::Unmarshalling {
            description: "Not Implemented".to_string(),
            protocol: CodecProtocol::Cbor,
        })
    }
    fn marshal_cbor(&self) -> Result<Vec<u8>, EncodingError> {
        // TODO
        Err(EncodingError::Marshalling {
            description: format!("Not implemented, data: {:?}", self),
            protocol: CodecProtocol::Cbor,
        })
    }
}
