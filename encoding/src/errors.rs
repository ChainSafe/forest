use std::fmt;

#[derive(Debug, PartialEq)]
pub enum Error {
    DecodingError(CodecProtocol),
    EncodingError(CodecProtocol),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Error::DecodingError(ref cdc) => write!(f, "Could not decode {}.", cdc),
            Error::EncodingError(ref cdc) => write!(f, "Could not encode {}.", cdc),
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum CodecProtocol {
    Cbor,
    JSON,
}

impl fmt::Display for CodecProtocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            CodecProtocol::Cbor => write!(f, "Cbor"),
            CodecProtocol::JSON => write!(f, "JSON"),
        }
    }
}
