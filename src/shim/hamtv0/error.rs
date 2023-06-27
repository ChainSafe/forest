// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::error::Error as StdError;

use fvm_ipld_encoding::Error as EncodingError;
use thiserror::Error;

/// HAMT Error
#[derive(Debug, Error)]
pub enum Error {
    /// Maximum depth error
    #[error("Maximum depth reached")]
    MaxDepth,
    /// Hash bits does not support greater than 8 bit width
    #[error("HashBits does not support retrieving more than 8 bits")]
    InvalidHashBitLen,
    /// Cid not found in store error
    #[error("Cid ({0}) did not match any in database")]
    CidNotFound(String),
    // TODO: This should be something like "internal" or "io". And we shouldn't have both this and
    // "other"; they serve the same purpose.
    /// Dynamic error for when the error needs to be forwarded as is.
    #[error("{0}")]
    Dynamic(anyhow::Error),
}

impl From<String> for Error {
    fn from(e: String) -> Self {
        Self::Dynamic(anyhow::anyhow!(e))
    }
}

impl From<&'static str> for Error {
    fn from(e: &'static str) -> Self {
        Self::Dynamic(anyhow::anyhow!(e))
    }
}

impl From<anyhow::Error> for Error {
    fn from(e: anyhow::Error) -> Self {
        e.downcast::<Error>().unwrap_or_else(Self::Dynamic)
    }
}

impl From<EncodingError> for Error {
    fn from(e: EncodingError) -> Self {
        Self::Dynamic(anyhow::anyhow!(e))
    }
}

impl From<Box<dyn StdError + Send + Sync>> for Error {
    fn from(e: Box<dyn StdError + Send + Sync>) -> Self {
        Self::Dynamic(anyhow::anyhow!(e))
    }
}
