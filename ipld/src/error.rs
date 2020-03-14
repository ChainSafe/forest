// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use serde::ser;
use std::fmt;

#[derive(Debug, PartialEq)]
pub enum Error {
    Encoding(String),
    Other(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Encoding(msg) => write!(f, "{}", msg),
            Error::Other(msg) => write!(f, "{}", msg),
        }
    }
}

impl ser::Error for Error {
    fn custom<T: fmt::Display>(msg: T) -> Error {
        Error::Encoding(msg.to_string())
    }
}

// TODO rework to handle low level source through io
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}
