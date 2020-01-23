// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use blocks::Error as BlkErr;
use encoding::{error::Error as SerdeErr, Error as EncErr};
use std::fmt;

#[derive(Debug, PartialEq)]
pub enum Error {
    NoBlocks,
    /// Error originating constructing blockchain structures
    BlkError(String),
    /// Error originating from encoding arbitrary data
    EncodingError(String),
    /// Error indicating an invalid root
    InvalidRoots,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NoBlocks => write!(f, "No blocks for tipset"),
            Error::InvalidRoots => write!(f, "Invalid root detected"),
            Error::BlkError(msg) => write!(
                f,
                "Error originating from construction of blockchain structures: {}",
                msg
            ),
            Error::EncodingError(msg) => write!(f, "Error originating from Encoding type: {}", msg),
        }
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
