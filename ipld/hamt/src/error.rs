// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use db::Error as DBError;
use std::fmt;

/// HAMT Error
#[derive(Debug, PartialEq)]
pub enum Error {
    /// Maximum depth error
    MaxDepth,
    /// Error interacting with underlying database
    Db(String),
    /// Custom HAMT error
    Custom(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::MaxDepth => write!(f, "Maximum depth reached"),
            Error::Db(msg) => write!(f, "Database Error: {}", msg),
            Error::Custom(msg) => write!(f, "Custom HAMT error: {}", msg),
        }
    }
}

impl From<DBError> for Error {
    fn from(e: DBError) -> Error {
        Error::Db(e.to_string())
    }
}
