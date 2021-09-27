// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use encoding::error::Error as CborError;
use serde::ser;
use std::fmt;
use thiserror::Error;

/// Ipld error
#[derive(Debug, PartialEq, Error)]
pub enum Error {
    #[error("{0}")]
    Encoding(String),
    #[error("{0}")]
    Other(&'static str),
    #[error("Failed to traverse link: {0}")]
    Link(String),
    #[error("{0}")]
    Custom(String),
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
