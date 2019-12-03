use super::Message;
use crypto::{Error as CryptoError, Signature, Signer};
use encoding::{Cbor, CodecProtocol, Error as EncodingError};

/// SignedMessage represents a wrapped message with signature bytes
#[derive(PartialEq, Clone, Debug)]
pub struct SignedMessage {
    pub(crate) message: Message,
    pub(crate) signature: Signature,
}

impl SignedMessage {
    pub fn new(msg: &Message, s: impl Signer) -> Result<SignedMessage, CryptoError> {
        let bz = msg.marshal_cbor()?;

        let sig = s.sign_bytes(bz, msg.from())?;

        Ok(SignedMessage {
            message: msg.clone(),
            signature: sig,
        })
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
