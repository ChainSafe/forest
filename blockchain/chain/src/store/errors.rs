// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use blocks::Error as BlkErr;
use cid::Error as CidErr;
use db::Error as DbErr;
use encoding::{error::Error as SerdeErr, Error as EncErr};
use ipld_amt::Error as AmtErr;
use std::error::Error as StdError;
use thiserror::Error;

/// Chain error
#[derive(Debug, Error)]
pub enum Error {
    /// Key was not found
    #[error("Invalid tipset: {0}")]
    UndefinedKey(String),
    /// Tipset contains no blocks
    #[error("No blocks for tipset")]
    NoBlocks,
    /// Key not found in database
    #[error("{0} not found")]
    NotFound(String),
    /// Error originating from key-value store
    #[error(transparent)]
    DB(#[from] DbErr),
    /// Error originating constructing blockchain structures
    #[error(transparent)]
    Blockchain(#[from] BlkErr),
    /// Error originating from encoding arbitrary data
    #[error("{0}")]
    Encoding(String),
    /// Error originating from Cid creation
    #[error(transparent)]
    Cid(#[from] CidErr),
    /// Amt error
    #[error("State error: {0}")]
    State(String),
    /// Other chain error
    #[error("{0}")]
    Other(String),
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
        Error::State(e.to_string())
    }
}

impl From<String> for Error {
    fn from(e: String) -> Self {
        Error::Other(e)
    }
}

impl From<Box<dyn StdError>> for Error {
    fn from(e: Box<dyn StdError>) -> Self {
        Error::Other(e.to_string())
    }
}
