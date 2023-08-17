// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

use crate::blocks::Tipset;
use crate::shim::clock::ChainEpoch;
#[cfg(test)]
use chrono::TimeZone;
use chrono::{DateTime, Duration, Utc};

/// Current state of the `ChainSyncer` using the `ChainExchange` protocol.
#[derive(PartialEq, Eq, Debug, Clone, Copy, strum::Display, strum::EnumString)]
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
pub enum SyncStage {
    /// Idle state.
    #[strum(to_string = "idle worker")]
    Idle,
    /// Syncing headers from the heaviest tipset to genesis.
    #[strum(to_string = "header sync")]
    Headers,
    /// Persisting headers on chain from heaviest to genesis.
    #[strum(to_string = "persisting headers")]
    PersistHeaders,
    /// Syncing messages and performing state transitions.
    #[strum(to_string = "message sync")]
    Messages,
    /// `ChainSync` completed and is following chain.
    #[strum(to_string = "complete")]
    Complete,
    #[cfg_attr(test, arbitrary(skip))]
    /// Error has occurred while syncing.
    #[strum(to_string = "error")]
    Error,
}

impl Default for SyncStage {
    fn default() -> Self {
        Self::Headers
    }
}

/// State of the node's syncing process.
/// This state is different from the general state of the `ChainSync` process.
#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
pub struct SyncState {
    base: Option<Arc<Tipset>>,
    target: Option<Arc<Tipset>>,

    stage: SyncStage,
    epoch: ChainEpoch,

    #[cfg_attr(test, arbitrary(gen(maybe_epoch0)))]
    start: Option<DateTime<Utc>>,
    #[cfg_attr(test, arbitrary(gen(maybe_epoch0)))]
    end: Option<DateTime<Utc>>,
    message: String,
}

#[cfg(test)]
fn maybe_epoch0(g: &mut quickcheck::Gen) -> Option<DateTime<Utc>> {
    match quickcheck::Arbitrary::arbitrary(g) {
        true => None,
        false => Some(Utc.timestamp_nanos(0)),
    }
}

impl SyncState {
    /// Initializes the syncing state with base and target tipsets and sets
    /// start time.
    pub fn init(&mut self, base: Arc<Tipset>, target: Arc<Tipset>) {
        *self = Self {
            target: Some(target),
            base: Some(base),
            start: Some(Utc::now()),
            ..Default::default()
        }
    }

    /// Get the current [`SyncStage`] of the `Syncer`
    pub fn stage(&self) -> SyncStage {
        self.stage
    }

    /// Returns the current [`Tipset`]
    pub fn target(&self) -> &Option<Arc<Tipset>> {
        &self.target
    }

    /// Return a reference to the base [`Tipset`]
    pub fn base(&self) -> &Option<Arc<Tipset>> {
        &self.base
    }

    /// Return the current [`ChainEpoch`]
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

    /// Sets the sync stage for the syncing state. If setting to complete, sets
    /// end timer to now.
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

mod lotus_json {
    use super::SyncState;
    use crate::{blocks::Tipset, chain_sync::SyncStage, lotus_json::*};
    use chrono::{DateTime, Utc};
    use std::sync::Arc;

    use serde::{Deserialize, Serialize};
    use serde_json::json;

    #[derive(Serialize, Deserialize)]
    #[serde(rename_all = "PascalCase")]
    pub struct SyncStateLotusJson {
        #[serde(skip_serializing_if = "LotusJson::is_none", default)]
        base: LotusJson<Option<Tipset>>,
        #[serde(skip_serializing_if = "LotusJson::is_none", default)]
        target: LotusJson<Option<Tipset>>,

        stage: LotusJson<SyncStage>,
        epoch: LotusJson<i64>,

        #[serde(skip_serializing_if = "LotusJson::is_none", default)]
        start: LotusJson<Option<DateTime<Utc>>>,
        #[serde(skip_serializing_if = "LotusJson::is_none", default)]
        end: LotusJson<Option<DateTime<Utc>>>,
        message: LotusJson<String>,
    }

    impl HasLotusJson for SyncState {
        type LotusJson = SyncStateLotusJson;

        fn snapshots() -> Vec<(serde_json::Value, Self)> {
            vec![(
                json!({
                    "Epoch": 0,
                    "Message": "",
                    "Stage": "header sync",
                }),
                Self::default(),
            )]
        }

        fn into_lotus_json(self) -> Self::LotusJson {
            let Self {
                base,
                target,
                stage,
                epoch,
                start,
                end,
                message,
            } = self;
            Self::LotusJson {
                base: base.as_deref().cloned().into(),
                target: target.as_deref().cloned().into(),
                stage: stage.into(),
                epoch: epoch.into(),
                start: start.into(),
                end: end.into(),
                message: message.into(),
            }
        }

        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
            let Self::LotusJson {
                base,
                target,
                stage,
                epoch,
                start,
                end,
                message,
            } = lotus_json;
            Self {
                base: base.into_inner().map(Arc::new),
                target: target.into_inner().map(Arc::new),
                stage: stage.into_inner(),
                epoch: epoch.into_inner(),
                start: start.into_inner(),
                end: end.into_inner(),
                message: message.into_inner(),
            }
        }
    }

    #[test]
    fn snapshots() {
        assert_all_snapshots::<SyncState>()
    }

    #[cfg(test)]
    quickcheck::quickcheck! {
        fn quickcheck(val: SyncState) -> () {
            assert_unchanged_via_json(val)
        }
    }
}
