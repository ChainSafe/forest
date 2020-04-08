// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use db::Error as DBError;
use forest_encoding::error::Error as CborError;
use forest_ipld::Error as IpldError;
use thiserror::Error;

/// HAMT Error
#[derive(Debug, PartialEq, Error)]
pub enum Error {
    /// Maximum depth error
    #[error("Maximum depth reached")]
    MaxDepth,
    /// Error interacting with underlying database
    #[error(transparent)]
    Db(#[from] DBError),
    /// Error encoding/ decoding values in store
    #[error("{0}")]
    Encoding(String),
    /// Custom HAMT error
    #[error("{0}")]
    Custom(&'static str),
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
