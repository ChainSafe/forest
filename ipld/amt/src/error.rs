// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Error as CidError;
use db::Error as DBError;
use encoding::error::Error as EncodingError;
use thiserror::Error;

/// AMT Error
#[derive(Debug, Error)]
pub enum Error {
    /// Index referenced it above arbitrary max set
    #[error("index {0} out of range for the amt")]
    OutOfRange(u64),
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
    /// Custom AMT error
    #[error("{0}")]
    Custom(&'static str),
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
            (&Custom(ref a), &Custom(ref b)) => a == b,
            _ => false,
        }
    }
}

impl From<Error> for String {
    fn from(e: Error) -> Self {
        e.to_string()
    }
}
