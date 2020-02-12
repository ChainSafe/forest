// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use amt::Error as AmtErr;
use std::fmt;

#[derive(Debug, PartialEq)]
pub enum Error {
    /// Error orginating from state
    State(String),
    /// Error originating from amt
    AMT(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::State(msg) => write!(f, "Error state data: {}", msg),
            Error::AMT(msg) => write!(f, "Error originating from the AMT: {}", msg),
        }
    }
}

impl From<AmtErr> for Error {
    fn from(e: AmtErr) -> Error {
        Error::AMT(e.to_string())
    }
}
