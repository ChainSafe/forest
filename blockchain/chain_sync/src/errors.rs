// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use amt::Error as AmtErr;
use blocks::Error as BlkErr;
use chain::Error as StoreErr;
use cid::Error as CidErr;
use db::Error as DbErr;
use encoding::{error::Error as SerdeErr, Error as EncErr};
use hamt::Error as HamtErr;
use state_manager::Error as StErr;
use thiserror::Error;

/// ChainSync error
#[derive(Debug, Error)]
pub enum Error {
    #[error("No blocks for tipset")]
    NoBlocks,
    /// Error originating constructing blockchain structures
    #[error(transparent)]
    Blockchain(#[from] BlkErr),
    /// Error originating from encoding arbitrary data
    #[error("{0}")]
    Encoding(String),
    /// Error originating from CID construction
    #[error(transparent)]
    InvalidCid(#[from] CidErr),
    /// Error indicating an invalid root
    #[error("Invalid message root detected")]
    InvalidRoots,
    /// Error indicating a chain store error
    #[error(transparent)]
    Store(#[from] StoreErr),
    /// Error originating from state
    #[error(transparent)]
    State(#[from] StErr),
    /// Error in validating arbitrary data
    #[error("{0}")]
    Validation(String),
    /// Block validated was from the future, this is intended to be a temporal error.
    #[error("Block received was from the future (now={0}, blk={1}")]
    Temporal(u64, u64),
    /// Any other error that does not need to be specifically handled
    #[error("{0}")]
    Other(String),
}

impl From<DbErr> for Error {
    fn from(e: DbErr) -> Error {
        Error::Store(e.into())
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

impl From<AmtErr> for Error {
    fn from(e: AmtErr) -> Error {
        Error::Other(e.to_string())
    }
}

impl From<HamtErr> for Error {
    fn from(e: HamtErr) -> Error {
        Error::Other(e.to_string())
    }
}

impl From<&str> for Error {
    fn from(e: &str) -> Error {
        Error::Other(e.to_string())
    }
}

impl From<String> for Error {
    fn from(e: String) -> Error {
        Error::Other(e)
    }
}

impl From<std::num::TryFromIntError> for Error {
    fn from(e: std::num::TryFromIntError) -> Error {
        Error::Other(e.to_string())
    }
}
