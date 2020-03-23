// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Error as CidError;
use db::Error as DBError;
use encoding::error::Error as EncodingError;
use std::fmt;

/// AMT Error
#[derive(Debug, PartialEq)]
pub enum Error {
    /// Index referenced it above arbitrary max set
    OutOfRange(u64),
    /// Cbor encoding error
    Cbor(String),
    /// Error generating a Cid for data
    Cid(String),
    /// Error interacting with underlying database
    Db(String),
    /// Error when trying to serialize an AMT without a flushed cache
    Cached,
    /// Custom AMT error
    Custom(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::OutOfRange(v) => write!(f, "index {} out of range for the amt", v),
            Error::Cbor(msg) => write!(f, "{}", msg),
            Error::Cid(msg) => write!(f, "{}", msg),
            Error::Db(msg) => write!(f, "{}", msg),
            Error::Cached => write!(
                f,
                "Tried to serialize without saving cache, run flush() on Amt before serializing"
            ),
            Error::Custom(msg) => write!(f, "Amt error: {}", msg),
        }
    }
}

impl From<Error> for String {
    fn from(e: Error) -> Self {
        e.to_string()
    }
}

impl From<DBError> for Error {
    fn from(e: DBError) -> Error {
        Error::Db(e.to_string())
    }
}

impl From<CidError> for Error {
    fn from(e: CidError) -> Error {
        Error::Cid(e.to_string())
    }
}

impl From<EncodingError> for Error {
    fn from(e: EncodingError) -> Error {
        Error::Cbor(e.to_string())
    }
}
