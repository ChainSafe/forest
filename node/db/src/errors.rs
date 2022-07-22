// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use encoding::error::Error as CborError;
use thiserror::Error;

/// Database error
#[derive(Debug, Error)]
pub enum Error {
    #[error("Invalid bulk write kv lengths, must be equal")]
    InvalidBulkLen,
    #[error("Cannot use unopened database")]
    Unopened,
    #[error(transparent)]
    Database(#[from] rocksdb::Error),
    #[error(transparent)]
    Encoding(#[from] CborError),
    #[error("{0}")]
    Other(String),
}

impl PartialEq for Error {
    fn eq(&self, other: &Self) -> bool {
        use Error::*;

        match (self, other) {
            (&InvalidBulkLen, &InvalidBulkLen) => true,
            (&Unopened, &Unopened) => true,
            (&Database(_), &Database(_)) => true,
            (&Encoding(_), &Encoding(_)) => true,
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
