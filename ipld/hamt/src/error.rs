// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use db::Error as DBError;
use forest_encoding::error::Error as CborError;
use forest_ipld::Error as IpldError;
use std::fmt;

/// HAMT Error
#[derive(Debug, PartialEq)]
pub enum Error {
    /// Maximum depth error
    MaxDepth,
    /// Error interacting with underlying database
    Db(String),
    /// Error encoding/ decoding values in store
    Encoding(String),
    /// Custom HAMT error
    Custom(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::MaxDepth => write!(f, "Maximum depth reached"),
            Error::Db(msg) => write!(f, "Database Error: {}", msg),
            Error::Encoding(msg) => write!(f, "Encoding Error: {}", msg),
            Error::Custom(msg) => write!(f, "HAMT error: {}", msg),
        }
    }
}

impl From<DBError> for Error {
    fn from(e: DBError) -> Error {
        Error::Db(e.to_string())
    }
}

impl From<CborError> for Error {
    fn from(e: CborError) -> Error {
        Error::Encoding(e.to_string())
    }
}

impl From<IpldError> for Error {
    fn from(e: IpldError) -> Error {
        Error::Encoding(e.to_string())
    }
}
