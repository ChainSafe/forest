use super::{BLS_PUB_LEN, PAYLOAD_HASH_LEN};
use data_encoding::DecodeError;
use std::num::ParseIntError;

#[derive(Debug, PartialEq)]
pub enum Error {
    UnknownNetwork,
    UnknownProtocol,
    InvalidPayload,
    InvalidLength,
    InvalidPayloadLength(usize),
    InvalidBLSLength(usize),
    InvalidChecksum,
    Base32Decoding(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Error::UnknownNetwork => write!(f, "Unknown address network"),
            Error::UnknownProtocol => write!(f, "Unknown address protocol"),
            Error::InvalidPayload => write!(f, "Invalid address payload"),
            Error::InvalidLength => write!(f, "Invalid address length"),
            Error::InvalidPayloadLength(ref len) => write!(
                f,
                "Invalid payload length, wanted: {} got: {}",
                PAYLOAD_HASH_LEN, len
            ),
            Error::InvalidBLSLength(ref len) => write!(
                f,
                "Invalid BLS pub key length, wanted: {} got: {}",
                BLS_PUB_LEN, len
            ),
            Error::InvalidChecksum => write!(f, "Invalid address checksum"),
            Error::Base32Decoding(ref err) => write!(f, "Decoding error: {}", err),
        }
    }
}

impl From<DecodeError> for Error {
    fn from(e: DecodeError) -> Error {
        Error::Base32Decoding(e.to_string())
    }
}
impl From<ParseIntError> for Error {
    fn from(_: ParseIntError) -> Error {
        Error::InvalidPayload
    }
}
