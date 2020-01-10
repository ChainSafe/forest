// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use std::fmt;

#[derive(Debug, PartialEq)]
pub enum Error {
    NoBlocks,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NoBlocks => write!(f, "No blocks for tipset"),
        }
    }
}
