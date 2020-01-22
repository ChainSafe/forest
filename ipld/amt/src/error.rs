// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use cid::Error as CidError;
use db::Error as DBError;
use std::fmt;

#[derive(Debug, PartialEq)]
pub enum Error {
    CidError(String),
    DbError(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::CidError(msg) => write!(f, "Error generating Cid: {}", msg),
            Error::DbError(msg) => write!(f, "Database Error: {}", msg),
        }
    }
}

impl From<DBError> for Error {
    fn from(e: DBError) -> Error {
        Error::DbError(e.to_string())
    }
}

impl From<CidError> for Error {
    fn from(e: CidError) -> Error {
        Error::CidError(e.to_string())
    }
}
