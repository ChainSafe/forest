// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use amt::Error as AmtErr;
use blocks::Error as BlkErr;
use chain::Error as StoreErr;
use cid::Error as CidErr;
use db::Error as DbErr;
use encoding::{error::Error as SerdeErr, Error as EncErr};
use state_manager::Error as StErr;
use std::fmt;

#[derive(Debug, PartialEq)]
pub enum Error {
    NoBlocks,
    /// Error originating constructing blockchain structures
    Blockchain(String),
    /// Error originating from encoding arbitrary data
    Encoding(String),
    /// Error originating from CID construction
    InvalidCid(String),
    /// Error indicating an invalid root
    InvalidRoots,
    /// Error indicating a chain store error
    Store(String),
    /// Error originating from key-value store
    KeyValueStore(String),
    /// Error originating from state
    State(String),
    /// Error in validating arbitrary data
    Validation(String),
    /// Any other error that does not need to be specifically handled
    Other(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NoBlocks => write!(f, "No blocks for tipset"),
            Error::InvalidRoots => write!(f, "Invalid root detected"),
            Error::Blockchain(msg) => write!(f, "{}", msg),
            Error::KeyValueStore(msg) => write!(f, "{}", msg),
            Error::Encoding(msg) => write!(f, "{}", msg),
            Error::InvalidCid(msg) => write!(f, "{}", msg),
            Error::Store(msg) => write!(f, "{}", msg),
            Error::State(msg) => write!(f, "{}", msg),
            Error::Validation(msg) => write!(f, "{}", msg),
            Error::Other(msg) => write!(f, "chain_sync error: {}", msg),
        }
    }
}

impl From<BlkErr> for Error {
    fn from(e: BlkErr) -> Error {
        Error::Blockchain(e.to_string())
    }
}

impl From<DbErr> for Error {
    fn from(e: DbErr) -> Error {
        Error::KeyValueStore(e.to_string())
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
        Error::InvalidCid(e.to_string())
    }
}

impl From<StoreErr> for Error {
    fn from(e: StoreErr) -> Error {
        Error::Store(e.to_string())
    }
}

impl From<AmtErr> for Error {
    fn from(e: AmtErr) -> Error {
        Error::Other(e.to_string())
    }
}

impl From<StErr> for Error {
    fn from(e: StErr) -> Error {
        Error::State(e.to_string())
    }
}

impl From<&str> for Error {
    fn from(e: &str) -> Error {
        Error::Other(e.to_string())
    }
}

impl From<std::num::TryFromIntError> for Error {
    fn from(e: std::num::TryFromIntError) -> Error {
        Error::Other(e.to_string())
    }
}
