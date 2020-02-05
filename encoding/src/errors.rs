// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use serde_cbor::error::Error as CborError;
use std::fmt;

/// Error type for encoding and decoding data through any Forest supported protocol
///
/// This error will provide any details about the data which was attempted to be
/// encoded or decoded. The
///
/// Usage:
/// ```no_run
/// use forest_encoding::{Error, CodecProtocol};
///
/// Error::Marshalling {
///     description: format!("{:?}", vec![0]),
///     protocol: CodecProtocol::Cbor,
/// };
/// Error::Unmarshalling {
///     description: format!("{:?}", vec![0]),
///     protocol: CodecProtocol::Cbor,
/// };
/// ```
#[derive(Debug, PartialEq)]
pub enum Error {
    Unmarshalling {
        description: String,
        protocol: CodecProtocol,
    },
    Marshalling {
        description: String,
        protocol: CodecProtocol,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Unmarshalling {
                description,
                protocol,
            } => write!(
                f,
                "Could not decode in format {}: {}",
                protocol, description
            ),
            Error::Marshalling {
                description,
                protocol,
            } => write!(
                f,
                "Could not encode in format {}: {}",
                protocol, description
            ),
        }
    }
}

impl From<CborError> for Error {
    fn from(err: CborError) -> Error {
        Error::Marshalling {
            description: err.to_string(),
            protocol: CodecProtocol::Cbor,
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
}

impl fmt::Display for CodecProtocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            CodecProtocol::Cbor => write!(f, "Cbor"),
        }
    }
}
