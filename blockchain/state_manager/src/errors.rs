// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_db::Error as DbErr;
use std::fmt::Debug;
use thiserror::Error;
use tokio::task::JoinError;

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

impl From<JoinError> for Error {
    fn from(e: JoinError) -> Self {
        Error::Other(format!("failed joining on tokio task: {e}"))
    }
}

impl From<fvm::kernel::ExecutionError> for Error {
    fn from(e: fvm::kernel::ExecutionError) -> Self {
        Error::Other(e.to_string())
    }
}
