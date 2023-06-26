// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[cfg(test)]
use thiserror::Error;

/// IPLD error
#[derive(Debug, PartialEq, Eq, Error)]
#[cfg(test)]
pub enum Error {
    #[error("Failed to traverse link: {0}")]
    Link(String),
    #[error("{0}")]
    Custom(String),
}
