// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use blocks::Tipset;
use clock::ChainEpoch;
use std::fmt;
use std::sync::Arc;
use std::time::SystemTime;

/// Current state of the ChainSyncer using the BlockSync protocol.
#[derive(PartialEq, Debug, Clone, Copy)]
pub enum SyncStage {
    /// Syncing headers from the heaviest tipset to genesis.
    Headers,
    /// Persisting headers on chain from heaviest to genesis.
    PersistHeaders,
    /// Syncing messages and performing state transitions.
    Messages,
    /// ChainSync completed and is following chain.
    Complete,
    /// Error has occured while syncing.
    Errored,
}

impl fmt::Display for SyncStage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SyncStage::Headers => write!(f, "header sync"),
            SyncStage::PersistHeaders => write!(f, "persisting headers"),
            SyncStage::Messages => write!(f, "message sync"),
            SyncStage::Complete => write!(f, "complete"),
            SyncStage::Errored => write!(f, "error"),
        }
    }
}

/// State of a given sync
#[derive(Clone, Debug)]
pub struct SyncState {
    target: Arc<Tipset>,
    base: Arc<Tipset>,
    stage: SyncStage,
    height: ChainEpoch,
    message: String,
    start: SystemTime,
    end: SystemTime,
}
