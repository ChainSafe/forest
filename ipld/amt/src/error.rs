// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use cid::Error as CidError;
use db::Error as DBError;
use encoding::Error as EncodingError;
use std::fmt;

#[derive(Debug, PartialEq)]
pub enum Error {
    OutOfRange(u64),
    Cbor(String),
    Cid(String),
    Db(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::OutOfRange(v) => write!(f, "index {} out of range for the amt", v),
            Error::Cbor(msg) => write!(f, "Could not (de)serialize object: {}", msg),
            Error::Cid(msg) => write!(f, "Error generating Cid: {}", msg),
            Error::Db(msg) => write!(f, "Database Error: {}", msg),
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

impl From<EncodingError> for Error {
    fn from(e: EncodingError) -> Error {
        Error::Cbor(e.to_string())
    }
}
