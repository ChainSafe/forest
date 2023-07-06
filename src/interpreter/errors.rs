// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocks;
use thiserror::Error;

/// Interpreter error.
#[derive(Debug, Error)]
pub enum Error {
    #[error("failed to read state from the database: {0}")]
    Lookup(#[from] anyhow::Error),

    #[error(transparent)]
    Signature(#[from] blocks::Error),
}
