// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use std::fmt;

#[derive(Debug, PartialEq)]
pub enum Error {
    /// Tipset contains invalid data, as described by the string parameter.
    InvalidTipSet(String),
    /// The given tipset has no blocks
    NoBlocks,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::InvalidTipSet(msg) => write!(f, "Invalid tipset: {}", msg),
            Error::NoBlocks => write!(f, "No blocks for tipset"),
        }
    }
}
