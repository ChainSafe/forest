// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::fmt::{Debug, Display};

use crate::shim::clock::ChainEpoch;
use thiserror::Error;
use tokio::task::JoinError;

/// State manager error
#[derive(Debug, PartialEq, Error)]
pub enum Error {
    /// Error originating from state
    #[error("{0}")]
    State(String),
    /// Refusing explicit call due to an expensive state migration at the requested epoch.
    #[error(
        "required historical state unavailable: refusing explicit call due to state fork at epoch {epoch}"
    )]
    ExpensiveFork { epoch: ChainEpoch },
    /// Sender doesn't exist or isn't a valid sender type.
    #[error("sender validation failed")]
    SenderValidationFailed,
    /// Other state manager error
    #[error("{0}")]
    Other(String),
}

/// Whether to enforce the FVM sender checks.
#[derive(Debug, Copy, Clone, Default, PartialEq, Eq)]
pub enum SenderValidation {
    /// Enforce the FVM explicit-path sender checks.
    #[default]
    Enforce,
    /// Skip sender validation for EVM contracts and non-existent senders.
    Skip,
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
        Error::other(format!("{e:#}"))
    }
}

impl From<JoinError> for Error {
    fn from(e: JoinError) -> Self {
        Error::Other(format!("failed joining on tokio task: {e}"))
    }
}
