use crate::unsigned_message::UnsignedMessage;

pub type FilCryptoSignature = String;

#[allow(dead_code)]
pub struct SignedMessage {
    message: UnsignedMessage,
    signature: FilCryptoSignature,
}
