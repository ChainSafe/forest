// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use blocks::{tipset::tipset_json::TipsetJsonRef, Tipset};
use clock::ChainEpoch;
use serde::{Serialize, Serializer};
use std::fmt;
use std::sync::Arc;
use std::time::SystemTime;

/// Current state of the ChainSyncer using the ChainExchange protocol.
#[derive(PartialEq, Debug, Clone, Copy)]
pub enum SyncStage {
    /// Idle state.
    Idle,
    /// Syncing headers from the heaviest tipset to genesis.
    Headers,
    /// Persisting headers on chain from heaviest to genesis.
    PersistHeaders,
    /// Syncing messages and performing state transitions.
    Messages,
    /// ChainSync completed and is following chain.
    Complete,
    /// Error has occured while syncing.
    Error,
}

impl Default for SyncStage {
    fn default() -> Self {
        Self::Headers
    }
}

impl fmt::Display for SyncStage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SyncStage::Idle => write!(f, "idle worker"),
            SyncStage::Headers => write!(f, "header sync"),
            SyncStage::PersistHeaders => write!(f, "persisting headers"),
            SyncStage::Messages => write!(f, "message sync"),
            SyncStage::Complete => write!(f, "complete"),
            SyncStage::Error => write!(f, "error"),
        }
    }
}

impl Serialize for SyncStage {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.to_string().serialize(serializer)
    }
}

/// State of a given sync. This state is used to keep track of the state of each sync worker.
/// This state is different from the general state of the ChainSync process.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SyncState {
    base: Option<Arc<Tipset>>,
    target: Option<Arc<Tipset>>,

    stage: SyncStage,
    epoch: ChainEpoch,

    start: Option<SystemTime>,
    end: Option<SystemTime>,
    message: String,
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

    /// Returns the current [Tipset] the
    pub fn target(&self) -> &Option<Arc<Tipset>> {
        &self.target
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
        self.stage = SyncStage::Error;
        self.end = Some(SystemTime::now());
    }
}

impl Serialize for SyncState {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        #[derive(Serialize)]
        #[serde(rename_all = "PascalCase")]
        struct SyncStateJson<'a> {
            base: Option<TipsetJsonRef<'a>>,
            target: Option<TipsetJsonRef<'a>>,

            stage: SyncStage,
            epoch: ChainEpoch,

            start: &'a Option<SystemTime>,
            end: &'a Option<SystemTime>,
            message: &'a str,
        }

        SyncStateJson {
            base: self.base.as_ref().map(|ts| TipsetJsonRef(ts.as_ref())),
            target: self.target.as_ref().map(|ts| TipsetJsonRef(ts.as_ref())),
            stage: self.stage,
            epoch: self.epoch,
            start: &self.start,
            end: &self.end,
            message: &self.message,
        }
        .serialize(serializer)
    }
}
