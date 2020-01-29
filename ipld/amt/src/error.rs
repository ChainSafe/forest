// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use cid::Error as CidError;
use db::Error as DBError;
use encoding::error;
use std::fmt;

#[derive(Debug, PartialEq)]
pub enum Error {
    OutOfRange(u64),
    Cbor(String),
    Cid(String),
    Db(String),
    Cached,
    Custom(String),
    InvalidAMT,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::OutOfRange(v) => write!(f, "index {} out of range for the amt", v),
            Error::Cbor(msg) => write!(f, "Could not (de)serialize object: {}", msg),
            Error::Cid(msg) => write!(f, "Error generating Cid: {}", msg),
            Error::Db(msg) => write!(f, "Database Error: {}", msg),
            Error::Cached => write!(
                f,
                "Tried to serialize without saving cache, run flush() on AMT before serializing"
            ),
            Error::Custom(msg) => write!(f, "Custom AMT error: {}", msg),
            Error::InvalidAMT => write!(f, "Invalid AMT, this is an unexpected error"),
        }
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

impl From<error::Error> for Error {
    fn from(e: error::Error) -> Error {
        Error::Cbor(e.to_string())
    }
}
