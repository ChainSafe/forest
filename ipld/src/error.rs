// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use serde::ser;
use std::fmt;
use thiserror::Error;

/// IPLD error
#[derive(Debug, PartialEq, Eq, Error)]
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
