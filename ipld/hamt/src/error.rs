// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use db::Error as DBError;
use forest_encoding::error::Error as CborError;
use forest_ipld::Error as IpldError;
use std::error::Error as StdError;
use thiserror::Error;

/// HAMT Error
#[derive(Debug, PartialEq, Error)]
pub enum Error {
    /// Maximum depth error
    #[error("Maximum depth reached")]
    MaxDepth,
    /// Hash bits does not support greater than 8 bit width
    #[error("HashBits does not support retrieving more than 8 bits")]
    InvalidHashBitLen,
    /// This should be treated as a fatal error, must have at least one pointer in node
    #[error("Invalid HAMT format, node cannot have 0 pointers")]
    ZeroPointers,
    /// Error interacting with underlying database
    #[error(transparent)]
    Db(#[from] DBError),
    /// Error encoding/ decoding values in store
    #[error("{0}")]
    Encoding(String),
    /// Cid not found in store error
    #[error("Cid ({0}) did not match any in database")]
    CidNotFound(String),
    /// Custom HAMT error
    #[error("{0}")]
    Other(String),
}

impl From<Error> for String {
    fn from(e: Error) -> Self {
        e.to_string()
    }
}

impl From<CborError> for Error {
    fn from(e: CborError) -> Error {
        Error::Encoding(e.to_string())
    }
}

impl From<IpldError> for Error {
    fn from(e: IpldError) -> Error {
        Error::Encoding(e.to_string())
    }
}

impl From<Box<dyn StdError>> for Error {
    fn from(e: Box<dyn StdError>) -> Self {
        Self::Other(e.to_string())
    }
}
