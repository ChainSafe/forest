// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::time::SystemTimeError as TimeErr;
use thiserror::Error;

/// Blockchain blocks error
#[derive(Debug, PartialEq, Error)]
pub enum Error {
    /// Tipset contains invalid data, as described by the string parameter.
    #[error("Invalid tipset: {0}")]
    InvalidTipset(String),
    /// The given tipset has no blocks
    #[error("No blocks for tipset")]
    NoBlocks,
    /// Invalid signature
    #[error("Invalid signature: {0}")]
    InvalidSignature(String),
    /// Error in validating arbitrary data
    #[error("Error validating data: {0}")]
    Validation(String),
}

impl From<TimeErr> for Error {
    fn from(e: TimeErr) -> Error {
        Error::Validation(e.to_string())
    }
}
