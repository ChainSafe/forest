// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{fmt, time::SystemTimeError as TimeErr};

#[derive(Debug, PartialEq)]
pub enum Error {
    /// Tipset contains invalid data, as described by the string parameter.
    InvalidTipSet(String),
    /// The given tipset has no blocks
    NoBlocks,
    /// Invalid signature
    InvalidSignature(String),
    /// Error in validating arbitrary data
    Validation(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::InvalidTipSet(msg) => write!(f, "Invalid tipset: {}", msg),
            Error::NoBlocks => write!(f, "No blocks for tipset"),
            Error::InvalidSignature(msg) => write!(f, "Invalid signature: {}", msg),
            Error::Validation(msg) => write!(f, "Error validating data: {}", msg),
        }
    }
}

impl From<TimeErr> for Error {
    fn from(e: TimeErr) -> Error {
        Error::Validation(e.to_string())
    }
}
