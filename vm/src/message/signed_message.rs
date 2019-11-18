use super::unsigned_message::UnsignedMessage;

pub type FilCryptoSignature = String;

#[allow(dead_code)]
pub struct SignedMessage {
    message: UnsignedMessage,
    signature: FilCryptoSignature,
}
