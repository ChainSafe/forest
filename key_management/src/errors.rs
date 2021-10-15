// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::io;
use thiserror::Error;

#[derive(Debug, PartialEq, Error)]
pub enum Error {
    /// info that corresponds to key does not exist
    #[error("Key info not found")]
    KeyInfo,
    /// Key already exists in keystore
    #[error("Key already exists")]
    KeyExists,
    #[error("Key does not exist")]
    KeyNotExists,
    #[error("Key not found")]
    NoKey,
    #[error("IO Error: {0}")]
    IO(String),
    #[error("{0}")]
    Other(String),
    #[error("Could not convert from KeyInfo to Key")]
    KeyInfoConversion,
}

impl From<io::Error> for Error {
    fn from(f: io::Error) -> Self {
        Error::IO(f.to_string())
    }
}
