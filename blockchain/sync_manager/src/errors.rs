// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use blocks::Error as BlkErr;
use std::fmt;

#[derive(Debug, PartialEq)]
pub enum Error {
    NoBlocks,
    /// Error originating constructing blockchain structures
    BlkError(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NoBlocks => write!(f, "No blocks for tipset"),
            Error::BlkError(msg) => write!(
                f,
                "Error originating from construction of blockchain structures: {}",
                msg
            ),
        }
    }
}

impl From<BlkErr> for Error {
    fn from(e: BlkErr) -> Error {
        Error::BlkError(e.to_string())
    }
}
