// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Error as CidError;
use db::Error as DBError;
use encoding::error::Error as EncodingError;
use std::error::Error as StdError;
use thiserror::Error;

/// AMT Error
#[derive(Debug, Error)]
pub enum Error {
    /// Index referenced it above arbitrary max set
    #[error("index {0} out of range for the amt")]
    OutOfRange(u64),
    /// Height of root node is greater than max.
    #[error("failed to load AMT: height out of bounds: {0} > {1}")]
    MaxHeight(u64, u64),
    /// Cbor encoding error
    #[error(transparent)]
    Encoding(#[from] EncodingError),
    /// Error generating a Cid for data
    #[error(transparent)]
    Cid(#[from] CidError),
    /// Error interacting with underlying database
    #[error(transparent)]
    DB(#[from] DBError),
    /// Error when trying to serialize an AMT without a flushed cache
    #[error("Tried to serialize without saving cache, run flush() on Amt before serializing")]
    Cached,
    /// Cid root was not found in underling data store
    #[error("Cid root not found in database")]
    RootNotFound,
    /// Serialized vector less than number of bits set
    #[error("Vector length does not match bitmap")]
    InvalidVecLength,
    /// Cid not found in store error
    #[error("Cid ({0}) did not match any in database")]
    CidNotFound(String),
    /// Custom AMT error
    #[error("{0}")]
    Other(String),
}

impl PartialEq for Error {
    fn eq(&self, other: &Self) -> bool {
        use Error::*;

        match (self, other) {
            (&OutOfRange(a), &OutOfRange(b)) => a == b,
            (&Encoding(_), &Encoding(_)) => true,
            (&Cid(ref a), &Cid(ref b)) => a == b,
            (&DB(ref a), &DB(ref b)) => a == b,
            (&Cached, &Cached) => true,
            (&Other(ref a), &Other(ref b)) => a == b,
            _ => false,
        }
    }
}

impl From<Error> for String {
    fn from(e: Error) -> Self {
        e.to_string()
    }
}

impl From<Box<dyn StdError>> for Error {
    fn from(e: Box<dyn StdError>) -> Self {
        Self::Other(e.to_string())
    }
}
