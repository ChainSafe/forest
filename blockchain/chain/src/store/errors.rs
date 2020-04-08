// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use blocks::Error as BlkErr;
use cid::Error as CidErr;
use db::Error as DbErr;
use encoding::{error::Error as SerdeErr, Error as EncErr};
use ipld_amt::Error as AmtErr;
use thiserror::Error;

/// Chain error
#[derive(Debug, Error, PartialEq)]
pub enum Error {
    /// Key was not found
    #[error("Invalid tipset: {0}")]
    UndefinedKey(String),
    /// Tipset contains no blocks
    #[error("No blocks for tipset")]
    NoBlocks,
    /// Key not found in database
    #[error("{0} not found")]
    NotFound(&'static str),
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
    #[error(transparent)]
    Amt(#[from] AmtErr),
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
