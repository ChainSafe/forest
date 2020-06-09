// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use thiserror::Error;

#[derive(Debug, PartialEq, Error)]
pub enum Error {
    /// info that corresponds to key does not exist
    #[error("Key info not found")]
    KeyInfo,
    /// Key already exists in keystore
    #[error("Key already exists")]
    KeyExists,
    #[error("Key does not exist")]
    KeyNotExists,
    #[error("Key not found")]
    NoKey,
    #[error("{0}")]
    Other(String),
    #[error("Could not convert from KeyInfo to Key")]
    KeyInfoConversion,
}
