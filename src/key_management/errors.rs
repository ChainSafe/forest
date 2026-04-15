// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::io;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    /// info that corresponds to key does not exist
    #[error("Key info not found")]
    KeyInfo,
    /// Key already exists in key store
    #[error("Key already exists")]
    KeyExists,
    #[error("Key does not exist")]
    KeyNotExists,
    #[error("Key not found")]
    NoKey,
    #[error(transparent)]
    Bls(#[from] bls_signatures::Error),
    #[error(transparent)]
    K256(#[from] k256::ecdsa::Error),
    #[error(transparent)]
    IO(#[from] io::Error),
    #[error("{0}")]
    Other(String),
    #[error("Could not convert from KeyInfo to Key")]
    KeyInfoConversion,
}

impl From<anyhow::Error> for Error {
    fn from(value: anyhow::Error) -> Self {
        Error::Other(value.to_string())
    }
}
