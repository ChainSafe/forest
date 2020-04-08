// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::multihash::DecodeOwnedError;
use thiserror::Error;

/// Car utility error
#[derive(Debug, Error)]
pub enum Error {
    #[error("Failed to parse CAR file: {0}")]
    ParsingError(String),
    #[error("Invalid CAR file: {0}")]
    InvalidFile(String),
    #[error("CAR error: {0}")]
    Other(String),
}

impl From<cid::Error> for Error {
    fn from(err: cid::Error) -> Error {
        Error::Other(err.to_string())
    }
}

impl From<DecodeOwnedError> for Error {
    fn from(err: DecodeOwnedError) -> Error {
        Error::ParsingError(err.to_string())
    }
}
