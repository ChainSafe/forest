// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::fmt::Debug;

use crate::db::Error as DbErr;
use thiserror::Error;
use tokio::task::JoinError;

/// State manager error
#[derive(Debug, PartialEq, Error)]
pub enum Error {
    /// Error originating from state
    #[error("{0}")]
    State(String),
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
