// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use blocks::{tipset::tipset_json::TipsetJsonRef, Tipset};
use chrono::{DateTime, Duration, Utc};
use clock::ChainEpoch;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::sync::Arc;

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

impl<'de> Deserialize<'de> for SyncStage {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let stage: &str = Deserialize::deserialize(deserializer)?;

        let output = match stage {
            "idle worker" => SyncStage::Idle,
            "header sync" => SyncStage::Headers,
            "persisting headers" => SyncStage::PersistHeaders,
            "message synce" => SyncStage::Messages,
            "complete" => SyncStage::Complete,
            _ => SyncStage::Error,
        };

        Ok(output)
    }
}

/// State of the node's syncing process.
/// This state is different from the general state of the ChainSync process.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SyncState {
    base: Option<Arc<Tipset>>,
    target: Option<Arc<Tipset>>,

    stage: SyncStage,
    epoch: ChainEpoch,

    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
    message: String,
}

impl SyncState {
    /// Initializes the syncing state with base and target tipsets and sets start time.
    pub fn init(&mut self, base: Arc<Tipset>, target: Arc<Tipset>) {
        *self = Self {
            target: Some(target),
            base: Some(base),
            start: Some(Utc::now()),
            ..Default::default()
        }
    }

    /// Get the current [SyncStage] of the Syncer
    pub fn stage(&self) -> SyncStage {
        self.stage
    }

    /// Returns the current [Tipset]
    pub fn target(&self) -> &Option<Arc<Tipset>> {
        &self.target
    }

    /// Return a reference to the base [Tipset]
    pub fn base(&self) -> &Option<Arc<Tipset>> {
        &self.base
    }

    /// Return the current [ChainEpoch]
    pub fn epoch(&self) -> ChainEpoch {
        self.epoch
    }

    /// Get the elapsed time of the current syncing process.
    /// Returns `None` if syncing has not started
    pub fn get_elapsed_time(&self) -> Option<Duration> {
        if let Some(start) = self.start {
            let elapsed_time = Utc::now() - start;
            Some(elapsed_time)
        } else {
            None
        }
    }

    /// Sets the sync stage for the syncing state. If setting to complete, sets end timer to now.
    pub fn set_stage(&mut self, stage: SyncStage) {
        if let SyncStage::Complete = stage {
            self.end = Some(Utc::now());
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
        self.end = Some(Utc::now());
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

            start: &'a Option<DateTime<Utc>>,
            end: &'a Option<DateTime<Utc>>,
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

impl<'de> Deserialize<'de> for SyncState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "PascalCase")]
        struct SyncStateDe {
            #[serde(with = "blocks::tipset_json")]
            base: Arc<Tipset>,
            #[serde(with = "blocks::tipset_json")]
            target: Arc<Tipset>,

            #[serde(with = "super::SyncStage")]
            stage: SyncStage,
            epoch: ChainEpoch,

            start: Option<DateTime<Utc>>,
            end: Option<DateTime<Utc>>,
            message: String,
        }

        let SyncStateDe {
            base,
            target,
            stage,
            epoch,
            start,
            end,
            message,
        } = Deserialize::deserialize(deserializer)?;
        Ok(SyncState {
            base: Some(base),
            target: Some(target),
            stage,
            epoch,
            start,
            end,
            message,
        })
    }
}

pub mod json {
    use super::SyncState;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Debug)]
    #[serde(transparent)]
    pub struct SyncStateJson(pub SyncState);

    #[derive(Serialize)]
    #[serde(transparent)]
    pub struct SyncStateRef<'a>(pub &'a SyncState);

    impl From<SyncStateJson> for SyncState {
        fn from(wrapper: SyncStateJson) -> Self {
            wrapper.0
        }
    }
}

pub mod vec {
    use serde::ser::SerializeSeq;

    use super::json::SyncStateRef;
    use super::*;

    #[derive(Serialize)]
    #[serde(transparent)]
    pub struct SyncStateJsonVec(#[serde(with = "self")] pub Vec<SyncState>);

    pub fn serialize<S>(m: &[SyncState], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(m.len()))?;
        for e in m {
            seq.serialize_element(&SyncStateRef(e))?;
        }
        seq.end()
    }
}
