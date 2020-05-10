// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use encoding::error::Error as CborError;
use serde::ser;
use std::error;
use std::fmt;
use thiserror::Error;

/// Ipld error
#[derive(Debug, PartialEq, Error)]
pub enum Error {
    #[error("{0}")]
    Encoding(String),
    #[error("{0}")]
    Other(&'static str),
    #[error("{0}")]
    Custom(String),
}

impl Error {
    pub fn new(msg: String) -> Self {
        Self::Custom(msg)
    }
}

impl From<Box<dyn error::Error>> for Error {
    fn from(err: Box<dyn error::Error>) -> Self {
        Self::Custom(err.to_string())
    }
}

impl From<&str> for Error {
    fn from(s: &str) -> Self {
        Self::Custom(s.to_owned())
    }
}

impl ser::Error for Error {
    fn custom<T: fmt::Display>(msg: T) -> Error {
        Error::Encoding(msg.to_string())
    }
}

impl From<CborError> for Error {
    fn from(e: CborError) -> Error {
        Error::Encoding(e.to_string())
    }
}
