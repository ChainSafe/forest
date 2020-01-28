// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use blocks::Error as BlkErr;
use cid::Error as CidErr;
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
    /// Error originating from key-value store
    KeyValueStore(String),
    /// Error originating constructing blockchain structures
    Blockchain(String),
    /// Error originating from encoding arbitrary data
    Encoding(String),
    /// Error originating from Cid creation
    Cid(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::UndefinedKey(msg) => write!(f, "Invalid key: {}", msg),
            Error::NoBlocks => write!(f, "No blocks for tipset"),
            Error::KeyValueStore(msg) => {
                write!(f, "Error originating from Key-Value store: {}", msg)
            }
            Error::Blockchain(msg) => write!(
                f,
                "Error originating from construction of blockchain structures: {}",
                msg
            ),
            Error::Encoding(msg) => write!(f, "Error originating from Encoding type: {}", msg),
            Error::Cid(msg) => write!(f, "Error originating from from Cid creation: {}", msg),
        }
    }
}

impl From<DbErr> for Error {
    fn from(e: DbErr) -> Error {
        Error::KeyValueStore(e.to_string())
    }
}

impl From<BlkErr> for Error {
    fn from(e: BlkErr) -> Error {
        Error::Blockchain(e.to_string())
    }
}

impl From<EncErr> for Error {
    fn from(e: EncErr) -> Error {
        Error::Encoding(e.to_string())
    }
}

impl From<SerdeErr> for Error {
    fn from(e: SerdeErr) -> Error {
        Error::Encoding(e.to_string())
    }
}

impl From<CidErr> for Error {
    fn from(e: CidErr) -> Error {
        Error::Cid(e.to_string())
    }
}
