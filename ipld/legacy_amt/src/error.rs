// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::anyhow;
use encoding::Error as EncodingError;
use forest_cid::Error as CidError;
use std::error::Error as StdError;
use thiserror::Error;

/// AMT Error
#[derive(Debug, Error)]
pub enum Error {
    /// Index referenced it above arbitrary max set
    #[error("index {0} out of range for the amt")]
    OutOfRange(usize),
    /// Height of root node is greater than max.
    #[error("failed to load AMT: height out of bounds: {0} > {1}")]
    MaxHeight(usize, usize),
    /// Error generating a Cid for data
    #[error(transparent)]
    Cid(#[from] CidError),
    /// Error when trying to serialize an AMT without a flushed cache
    #[error("Tried to serialize without saving cache, run flush() on Amt before serializing")]
    Cached,
    /// Serialized vector less than number of bits set
    #[error("Vector length does not match bitmap")]
    InvalidVecLength,
    /// Invalid formatted serialized node.
    #[error("Serialized node cannot contain both links and values")]
    LinksAndValues,
    /// Cid not found in store error
    #[error("Cid ({0}) did not match any in database")]
    CidNotFound(String),
    /// Dynamic error for when the error needs to be forwarded as is.
    #[error("{0}")]
    Dynamic(anyhow::Error),
    /// Custom AMT error
    #[error("{0}")]
    Other(String),
}

impl From<EncodingError> for Error {
    fn from(e: EncodingError) -> Self {
        Self::Dynamic(anyhow!(e))
    }
}

impl From<Box<dyn StdError + Send + Sync>> for Error {
    fn from(e: Box<dyn StdError + Send + Sync>) -> Self {
        Self::Dynamic(anyhow!(e))
    }
}

impl From<anyhow::Error> for Error {
    fn from(e: anyhow::Error) -> Self {
        e.downcast::<Error>().unwrap_or_else(Self::Dynamic)
    }
}
