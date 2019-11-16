use crate::unsigned_message::UnsignedMessage;

pub type FilCryptoSignature = String;

pub struct SignedMessage {
    pub message: UnsignedMessage,
    pub signature: FilCryptoSignature,
}
