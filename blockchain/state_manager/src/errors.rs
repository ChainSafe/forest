// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use db::Error as DbErr;
use thiserror::Error;

/// State manager error
#[derive(Debug, PartialEq, Error)]
pub enum Error {
    /// Error orginating from state
    #[error("{0}")]
    State(String),
    /// Actor for given address not found
    #[error("Actor for address: {0} does not exist")]
    ActorNotFound(String),
    /// Actor state not found at given cid
    #[error("Actor state with cid {0} not found")]
    ActorStateNotFound(String),
    /// Error originating from key-value store
    #[error(transparent)]
    DB(#[from] DbErr),
}
