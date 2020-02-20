// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use db::Error as DbErr;
use std::fmt;

#[derive(Debug, PartialEq)]
pub enum Error {
    /// Error orginating from state
    State(String),
    /// Error originating from key-value store
    KeyValueStore(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::State(msg) => write!(f, "{}", msg),
            Error::KeyValueStore(msg) => {
                write!(f, "Error originating from Key-Value store: {}", msg)
            }
        }
    }
}

impl From<DbErr> for Error {
    fn from(e: DbErr) -> Error {
        Error::KeyValueStore(e.to_string())
    }
}
