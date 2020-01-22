// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use blocks::Error as BlkErr;
use db::Error as DbErr;
use encoding::{error::Error as SerdeErr, Error as EncErr};
use serde::Deserialize;
use std::fmt;

#[derive(Debug, PartialEq, Deserialize)]
pub enum Error {
    /// Key was not found
    UndefinedKey(String),
    /// Tipset contains no blocks
    NoBlocks,
    /// Keys are already written to store
    KeyExists,
    /// Error originating from key-value store
    KVError(String),
    /// Error originating constructing blockchain structures
    BlkError(String),
    /// Error originating from encoding arbitrary data
    EncodingError(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::UndefinedKey(msg) => write!(f, "Invalid key: {}", msg),
            Error::NoBlocks => write!(f, "No blocks for tipset"),
            Error::KeyExists => write!(f, "Keys already exist in store"),
            Error::KVError(msg) => write!(f, "Error originating from Key-Value store: {}", msg),
            Error::BlkError(msg) => write!(
                f,
                "Error originating from construction of blockchain structures: {}",
                msg
            ),
            Error::EncodingError(msg) => write!(f, "Error originating from Encoding type: {}", msg),
        }
    }
}

impl From<DbErr> for Error {
    fn from(e: DbErr) -> Error {
        Error::KVError(e.to_string())
    }
}

impl From<BlkErr> for Error {
    fn from(e: BlkErr) -> Error {
        Error::BlkError(e.to_string())
    }
}

impl From<EncErr> for Error {
    fn from(e: EncErr) -> Error {
        Error::EncodingError(e.to_string())
    }
}

impl From<SerdeErr> for Error {
    fn from(e: SerdeErr) -> Error {
        Error::EncodingError(e.to_string())
    }
}
