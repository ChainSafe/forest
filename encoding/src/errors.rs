// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Error as CidError;
use serde_cbor::error::Error as CborError;
use std::fmt;
use std::io;
use thiserror::Error;

/// Error type for encoding and decoding data through any Forest supported protocol.
///
/// This error will provide any details about the data which was attempted to be
/// encoded or decoded.
#[derive(Debug, PartialEq, Error)]
#[error("Serialization error for {protocol} protocol: {description}")]
pub struct Error {
    pub description: String,
    pub protocol: CodecProtocol,
}

impl From<CborError> for Error {
    fn from(err: CborError) -> Error {
        Self {
            description: err.to_string(),
            protocol: CodecProtocol::Cbor,
        }
    }
}

impl From<CidError> for Error {
    fn from(err: CidError) -> Self {
        Self {
            description: err.to_string(),
            protocol: CodecProtocol::Cbor,
        }
    }
}

impl From<Error> for io::Error {
    fn from(err: Error) -> Self {
        Self::new(io::ErrorKind::Other, err)
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
