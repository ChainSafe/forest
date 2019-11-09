use crate::unsigned_message::{UnsignedMessage};

pub type FilCryptoSignature = String; 

pub struct SignedMessage {
    message: UnsignedMessage,
    signature: FilCryptoSignature
}