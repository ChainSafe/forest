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

impl Default for SyncStage {
    fn default() -> Self {
        Self::Headers
    }
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

/// State of a given sync.
#[derive(Clone, Debug, Default)]
pub struct SyncState {
    target: Option<Arc<Tipset>>,
    base: Option<Arc<Tipset>>,
    stage: SyncStage,
    epoch: ChainEpoch,
    message: String,
    start: Option<SystemTime>,
    end: Option<SystemTime>,
}

impl SyncState {
    /// Initializes the syncing state with base and target tipsets and sets start time.
    pub fn init(&mut self, base: Arc<Tipset>, target: Arc<Tipset>) {
        *self = Self {
            target: Some(target),
            base: Some(base),
            start: Some(SystemTime::now()),
            ..Default::default()
        }
    }

    pub fn stage(&self) -> SyncStage {
        self.stage
    }

    /// Sets the sync stage for the syncing state. If setting to complete, sets end timer to now.
    pub fn set_stage(&mut self, stage: SyncStage) {
        if let SyncStage::Complete = stage {
            self.end = Some(SystemTime::now());
        }
        self.stage = stage;
    }

    /// Sets epoch of the sync.
    pub fn set_epoch(&mut self, epoch: ChainEpoch) {
        self.epoch = epoch;
    }

    /// Sets error for the sync.
    pub fn error(&mut self, err: String) {
        self.message = err;
        self.stage = SyncStage::Errored;
        self.end = Some(SystemTime::now());
    }
}
