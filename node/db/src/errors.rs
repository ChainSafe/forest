// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use encoding::error::Error as CborError;
use std::fmt;

#[derive(Debug, PartialEq)]
pub enum Error {
    InvalidBulkLen,
    Database(String),
    Encoding(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::InvalidBulkLen => write!(f, "Invalid bulk write kv lengths, must be equal"),
            Error::Database(msg) => write!(f, "{}", msg),
            Error::Encoding(msg) => write!(f, "{}", msg),
        }
    }
}

impl From<rocksdb::Error> for Error {
    fn from(e: rocksdb::Error) -> Error {
        Error::Database(String::from(e))
    }
}

impl From<CborError> for Error {
    fn from(e: CborError) -> Error {
        Error::Encoding(e.to_string())
    }
}
