// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use thiserror::Error;

/// Database error
#[derive(Debug, Error)]
pub enum Error {
    #[error("Invalid bulk write kv lengths, must be equal")]
    InvalidBulkLen,
    #[error("Cannot use unopened database")]
    Unopened,
    #[cfg(feature = "rocksdb")]
    #[error(transparent)]
    Database(#[from] rocksdb::Error),
    #[cfg(feature = "paritydb")]
    #[error(transparent)]
    Database(#[from] parity_db::Error),
    #[error("{0}")]
    Other(String),
}

impl PartialEq for Error {
    fn eq(&self, other: &Self) -> bool {
        use Error::*;

        match (self, other) {
            (&InvalidBulkLen, &InvalidBulkLen) => true,
            (&Unopened, &Unopened) => true,
            #[cfg(feature = "rocksdb")]
            (&Database(_), &Database(_)) => true,
            #[cfg(feature = "paritydb")]
            (&Database(_), &Database(_)) => true,
            (Other(a), Other(b)) => a == b,
            _ => false,
        }
    }
}

impl From<Error> for String {
    fn from(e: Error) -> Self {
        e.to_string()
    }
}
