// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::fmt;

#[derive(Debug, PartialEq)]
pub enum Error {
    /// Error orginating from state
    State(String),
    /// Error originating from encoding arbitrary data
    Encoding(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::State(msg) => write!(f, "Error state data: {}", msg),
            Error::Encoding(msg) => write!(f, "Error originating from Encoding type: {}", msg),
        }
    }
}
