// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use thiserror::Error;

/// Database error
#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Database(#[from] crate::db::db_engine::DbError),
}
