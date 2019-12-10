use super::{Message, UnsignedMessage};
use vm::{MethodNum, MethodParams, TokenAmount};

use address::Address;
use crypto::{Error as CryptoError, Signature, Signer};
use encoding::{Cbor, CodecProtocol, Error as EncodingError};
use num_bigint::BigUint;

/// SignedMessage represents a wrapped message with signature bytes
#[derive(PartialEq, Clone, Debug)]
pub struct SignedMessage {
    message: UnsignedMessage,
    signature: Signature,
}

impl SignedMessage {
    pub fn new(msg: &UnsignedMessage, s: &impl Signer) -> Result<SignedMessage, CryptoError> {
        let bz = msg.marshal_cbor()?;

        let sig = s.sign_bytes(bz, msg.from())?;

        Ok(SignedMessage {
            message: msg.clone(),
            signature: sig,
        })
    }
    pub fn message(&self) -> UnsignedMessage {
        self.message.clone()
    }
    pub fn signature(&self) -> Signature {
        self.signature.clone()
    }
}

impl Message for SignedMessage {
    /// from returns the from address of the message
    fn from(&self) -> Address {
        self.message.from()
    }
    /// to returns the destination address of the message
    fn to(&self) -> Address {
        self.message.to()
    }
    /// sequence returns the message sequence or nonce
    fn sequence(&self) -> u64 {
        self.message.sequence()
    }
    /// value returns the amount sent in message
    fn value(&self) -> TokenAmount {
        self.message.value()
    }
    /// method_num returns the method number to be called
    fn method_num(&self) -> MethodNum {
        self.message.method_num()
    }
    /// params returns the encoded parameters for the method call
    fn params(&self) -> MethodParams {
        self.message.params()
    }
    /// gas_price returns gas price for the message
    fn gas_price(&self) -> BigUint {
        self.message.gas_price()
    }
    /// gas_limit returns the gas limit for the message
    fn gas_limit(&self) -> BigUint {
        self.message.gas_limit()
    }
}

impl Cbor for SignedMessage {
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
