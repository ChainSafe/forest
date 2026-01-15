// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::fmt::{Debug, Display};

use thiserror::Error;
use tokio::task::JoinError;

/// State manager error
#[derive(Debug, PartialEq, Error)]
pub enum Error {
    /// Error originating from state
    #[error("{0}")]
    State(String),
    /// Other state manager error
    #[error("{0}")]
    Other(String),
    #[error("refusing explicit call due to expensive fork")]
    ExpensiveFork,
}

impl Error {
    pub fn state(e: impl Display) -> Self {
        Self::State(e.to_string())
    }

    pub fn other(e: impl Display) -> Self {
        Self::Other(e.to_string())
    }
}

impl From<String> for Error {
    fn from(e: String) -> Self {
        Error::Other(e)
    }
}

impl From<anyhow::Error> for Error {
    fn from(e: anyhow::Error) -> Self {
        Error::other(e)
    }
}

impl From<JoinError> for Error {
    fn from(e: JoinError) -> Self {
        Error::Other(format!("failed joining on tokio task: {e}"))
    }
}
