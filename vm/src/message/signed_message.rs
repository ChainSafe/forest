use super::errors::Error;
use super::signature::Signature;
use super::signer::Signer;
use super::Message;

/// SignedMessage represents a wrapped message with signature bytes
#[derive(PartialEq, Clone)]
pub struct SignedMessage {
    pub(crate) message: Message,
    pub(crate) signature: Signature,
}

impl SignedMessage {
    pub fn new(msg: &Message, s: impl Signer) -> Result<SignedMessage, Error> {
        let bz = msg.marshall_cbor()?;

        let sig = s.sign_bytes(bz, msg.from.clone())?;

        Ok(SignedMessage {
            message: msg.clone(),
            signature: sig,
        })
    }

    // Marshalling and unmarshalling
    pub fn unmarshall_cbor(&mut self, _bz: &mut [u8]) -> Result<(), Error> {
        // TODO
        Err(Error::DecodingError)
    }
    pub fn marshall_cbor(&self) -> Result<Vec<u8>, Error> {
        // TODO
        Err(Error::EncodingError)
    }
}
