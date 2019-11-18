use super::{BLS_PUB_LEN, PAYLOAD_HASH_LEN};

#[derive(Debug, PartialEq)]
pub enum AddressError {
    UnknownNetwork,
    UnknownProtocol,
    InvalidPayload,
    InvalidLength,
    InvalidPayloadLength(usize),
    InvalidBLSLength(usize),
    InvalidChecksum,
    Base32Decoding(String),
}

impl std::fmt::Display for AddressError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            AddressError::UnknownNetwork => write!(f, "Unknown address network"),
            AddressError::UnknownProtocol => write!(f, "Unknown address protocol"),
            AddressError::InvalidPayload => write!(f, "Invalid address payload"),
            AddressError::InvalidLength => write!(f, "Invalid address length"),
            AddressError::InvalidPayloadLength(ref len) => write!(
                f,
                "Invalid payload length, wanted: {} got: {}",
                PAYLOAD_HASH_LEN, len
            ),
            AddressError::InvalidBLSLength(ref len) => write!(
                f,
                "Invalid BLS pub key length, wanted: {} got: {}",
                BLS_PUB_LEN, len
            ),
            AddressError::InvalidChecksum => write!(f, "Invalid address checksum"),
            AddressError::Base32Decoding(ref err) => write!(f, "Decoding error: {}", err),
        }
    }
}
