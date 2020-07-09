// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::fmt;

/// Current state of the ChainSyncer using the BlockSync protocol
#[derive(PartialEq, Debug, Clone, Copy)]
pub enum SyncState {
    /// Syncing headers from the heaviest tipset to genesis
    Headers,
    /// Persisting headers on chain from heaviest to genesis
    PersistHeaders,
    /// Syncing messages and performing state transitions
    Messages,
    /// ChainSync completed and is following chain.
    Complete,
    /// Error has occured while syncing
    Errored,
}

impl fmt::Display for SyncState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SyncState::Headers => write!(f, "header sync"),
            SyncState::PersistHeaders => write!(f, "persisting headers"),
            SyncState::Messages => write!(f, "message sync"),
            SyncState::Complete => write!(f, "complete"),
            SyncState::Errored => write!(f, "error"),
        }
    }
}
