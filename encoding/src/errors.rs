use std::fmt;

/// Error type for encoding and decoding data through any Ferret supported protocol
///
/// This error will provide any details about the data which was attempted to be
/// encoded or decoded. The
///
/// Usage:
/// ```no_run
/// use encoding::{Error, CodecProtocol};
///
/// Error::Marshalling {
///     formatted_data: format!("{:?}", vec![0]),
///     protocol: CodecProtocol::Cbor,
/// };
/// Error::Unmarshalling {
///     formatted_data: format!("{:?}", vec![0]),
///     protocol: CodecProtocol::JSON,
/// };
/// ```
#[derive(Debug, PartialEq)]
pub enum Error {
    Unmarshalling {
        formatted_data: String,
        protocol: CodecProtocol,
    },
    Marshalling {
        formatted_data: String,
        protocol: CodecProtocol,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Unmarshalling {
                formatted_data,
                protocol,
            } => write!(
                f,
                "Could not decode: {} in format: {}.",
                formatted_data, protocol
            ),
            Error::Marshalling {
                formatted_data,
                protocol,
            } => write!(
                f,
                "Could not encode: {} in format: {}.",
                formatted_data, protocol
            ),
        }
    }
}

/// CodecProtocol defines the protocol in which the data is encoded or decoded
///
/// This is used with the encoding errors, to detail the encoding protocol or any other
/// information about how the data was encoded or decoded
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
