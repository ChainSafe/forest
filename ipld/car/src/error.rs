// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::multihash::DecodeOwnedError;
use std::fmt;

#[derive(Debug)]
pub enum Error {
    ParsingError(String),
    InvalidFile(String),
    Other(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::ParsingError(err) => write!(f, "Failed to parse CAR file: {}", err.clone()),
            Error::InvalidFile(err) => write!(f, "Invalid CAR file: {}", err.clone()),
            Error::Other(err) => write!(f, "CAR Error: {}", err.clone()),
        }
    }
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
