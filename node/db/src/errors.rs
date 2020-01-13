// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use rocksdb;
use std::fmt;

#[derive(Debug, PartialEq)]
pub struct Error {
    msg: String,
}

impl Error {
    pub fn new(msg: String) -> Self {
        Self { msg }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Database error: {}", self.msg)
    }
}

impl From<rocksdb::Error> for Error {
    fn from(e: rocksdb::Error) -> Error {
        Error {
            msg: String::from(e),
        }
    }
}
