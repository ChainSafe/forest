// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use blocks::Error as BlkErr;
use cid::Error as CidErr;
use encoding::{error::Error as SerdeErr, Error as EncErr};
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
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NoBlocks => write!(f, "No blocks for tipset"),
            Error::InvalidRoots => write!(f, "Invalid root detected"),
            Error::Blockchain(msg) => write!(
                f,
                "Error originating from construction of blockchain structures: {}",
                msg
            ),
            Error::Encoding(msg) => write!(f, "Error originating from Encoding type: {}", msg),
            Error::InvalidCid(msg) => write!(f, "Error originating from CID construction: {}", msg),
        }
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
        Error::InvalidCid(e.to_string())
    }
}
