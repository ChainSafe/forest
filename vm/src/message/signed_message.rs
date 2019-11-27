use super::Message;
use crypto::{Error as CryptoError, Signature, Signer};
use encoding::Cbor;

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
}

impl Cbor for SignedMessage {
    fn unmarshall_cbor(&mut self, _bz: &mut [u8]) -> Result<(), String> {
        // TODO
        Err("Unmarshall is unimplemented".to_owned())
    }
    fn marshall_cbor(&self) -> Result<Vec<u8>, String> {
        // TODO
        Err("Marshall is unimplemented".to_owned())
    }
}
