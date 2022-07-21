// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_db::Error as DbErr;
use thiserror::Error;

/// State manager error
#[derive(Debug, PartialEq, Error)]
pub enum Error {
    /// Error originating from state
    #[error("{0}")]
    State(String),
    /// Error from VM execution
    #[error("{0}")]
    VM(String),
    /// Actor for given address not found
    #[error("Actor for address: {0} does not exist")]
    ActorNotFound(String),
    /// Actor state not found at given CID
    #[error("Actor state with cid {0} not found")]
    ActorStateNotFound(String),
    /// Error originating from key-value store
    #[error(transparent)]
    DB(#[from] DbErr),
    /// Other state manager error
    #[error("{0}")]
    Other(String),
}

impl From<String> for Error {
    fn from(e: String) -> Self {
        Error::Other(e)
    }
}
impl From<anyhow::Error> for Error {
    fn from(e: anyhow::Error) -> Self {
        Error::Other(e.to_string())
    }
}

impl From<forest_encoding::error::Error> for Error {
    fn from(e: forest_encoding::error::Error) -> Self {
        Error::Other(e.to_string())
    }
}

impl From<fvm::kernel::ExecutionError> for Error {
    fn from(e: fvm::kernel::ExecutionError) -> Self {
        Error::Other(e.to_string())
    }
}
