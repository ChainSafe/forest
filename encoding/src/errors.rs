// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Error as CidError;
use serde_cbor::error::Error as CborError;
use std::fmt;
use thiserror::Error;

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
#[derive(Debug, PartialEq, Error)]
pub enum Error {
    #[error("Could not decode in format {protocol}: {description}")]
    Unmarshalling {
        description: String,
        protocol: CodecProtocol,
    },
    #[error("Could not encode in format {protocol}: {description}")]
    Marshalling {
        description: String,
        protocol: CodecProtocol,
    },
}

impl From<CborError> for Error {
    fn from(err: CborError) -> Error {
        Error::Marshalling {
            description: err.to_string(),
            protocol: CodecProtocol::Cbor,
        }
    }
}

impl From<CidError> for Error {
    fn from(err: CidError) -> Error {
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
