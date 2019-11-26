use super::Message;
use crypto::{Error as CryptoError, Signature, Signer};

/// SignedMessage represents a wrapped message with signature bytes
#[derive(PartialEq, Clone)]
pub struct SignedMessage {
    pub(crate) message: Message,
    pub(crate) signature: Signature,
}

impl SignedMessage {
    pub fn new(msg: &Message, s: impl Signer) -> Result<SignedMessage, CryptoError> {
        let bz = msg.marshall_cbor()?;

        let sig = s.sign_bytes(bz, msg.from())?;

        Ok(SignedMessage {
            message: msg.clone(),
            signature: sig,
        })
    }

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
