// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use blocks::{
    tipset::tipset_json::{TipsetJson, TipsetJsonRef},
    Tipset,
};
use clock::ChainEpoch;
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::sync::Arc;
use std::time::{SystemTime, Duration};
use num_enum::TryFromPrimitive;
use std::convert::TryFrom;
use chrono::prelude::*;
use chrono::naive::NaiveDateTime;
use chrono::offset::Utc;
use chrono::format::ParseResult;

// {YEAR}-{MONTH}-{DAY}T{HOUR}:{MINUTE}:{SECONDS}Z
// Ex  2020-05-03T:05:30:00
pub const DATE_TIME_FORMAT : &str = "%Y-%m-%dT%H:%M:%SZ";

pub fn get_naive_time_now() -> NaiveDateTime {
    let now = Utc::now();
    NaiveDateTime::new(now.date().naive_utc(), now.time())
}


/// Current state of the ChainSyncer using the BlockSync protocol.
#[derive(TryFromPrimitive)]
#[repr(u64)]
#[derive(PartialEq, Debug, Clone, Copy, Deserialize)]
pub enum SyncStage {
    /// Syncing headers from the heaviest tipset to genesis.
    #[serde(rename(deserialize = "header sync"))]
    Headers,
    /// Persisting headers on chain from heaviest to genesis.
    #[serde(rename(deserialize = "persisting headers"))]
    PersistHeaders,
    /// Syncing messages and performing state transitions.
    #[serde(rename(deserialize = "message sync"))]
    Messages,
    /// ChainSync completed and is following chain.
    #[serde(rename(deserialize = "complete"))]
    Complete,
    /// Error has occured while syncing.
    #[serde(rename(deserialize = "error"))]
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

/// State of a given sync.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SyncState {
    pub base: Option<Arc<Tipset>>,
    pub target: Option<Arc<Tipset>>,

    stage: SyncStage,

    pub epoch: ChainEpoch,
    pub start: Option<NaiveDateTime>,
    pub end: Option<NaiveDateTime>,
    pub message: String,
}

impl SyncState {
    /// Initializes the syncing state with base and target tipsets and sets start time.
    pub fn init(&mut self, base: Arc<Tipset>, target: Arc<Tipset>) {
        let now = Utc::now();
        *self = Self {
            target: Some(target),
            base: Some(base),
            start: Some(get_naive_time_now()),
            ..Default::default()
        }
    }

    pub fn stage(&self) -> SyncStage {
        self.stage
    }

    /// Sets the sync stage for the syncing state. If setting to complete, sets end timer to now.
    pub fn set_stage(&mut self, stage: SyncStage) {
        let now = Utc::now();
        if let SyncStage::Complete = stage {
            self.end = Some(get_naive_time_now());
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
        let now = Utc::now();
        self.end = Some(get_naive_time_now());
    }
}

fn format_se_date_time(s : Option<NaiveDateTime>) -> String{
    s.map(|d| d.format(DATE_TIME_FORMAT).to_string()).unwrap_or_default()
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

            stage: u64 ,
            height: ChainEpoch,

            start: &'a String,
            end: &'a String,
            message: &'a str,
        }

        SyncStateJson {
            base: self.base.as_ref().map(|ts| TipsetJsonRef(ts.as_ref())),
            target: self.target.as_ref().map(|ts| TipsetJsonRef(ts.as_ref())),
            stage: self.stage as u64,
            height: self.epoch,
            start: &format_se_date_time(self.start),
            end: &format_se_date_time(self.end),
            message: &self.message,
        }
        .serialize(serializer)
    }
}

fn format_de_date_time(s : String) -> ParseResult<Option<NaiveDateTime>>{
    NaiveDateTime::parse_from_str (&s, DATE_TIME_FORMAT).map(|i| Some(i))
}

impl<'de> Deserialize<'de> for SyncState {
    fn deserialize<D>(deserializer: D) -> Result<Self, <D as Deserializer<'de>>::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "PascalCase")]
        struct SyncStateJson {
            base: Option<TipsetJson>,
            target: Option<TipsetJson>,
            stage: u64,
            height: i64,
            start: Option<String>,
            end: Option<String>,
            message: String,
        }
        let SyncStateJson {
            base,
            target,
            stage,
            height,
            start,
            end,
            message,
        } = Deserialize::deserialize(deserializer)?;
    
        let start_naive_date_time = start.map_or(Ok(None),  |s| format_de_date_time(s)).map_err(de::Error::custom) ?;
        let end_naive_date_time = end.map_or(Ok(None),  |s| format_de_date_time(s)).map_err(de::Error::custom) ?;
        let stage_num = SyncStage::try_from(stage).map_err(de::Error::custom) ?;
                
        Ok(Self {
            base: base.map(|b| Arc::new(b.0)),
            target: target.map(|b| Arc::new(b.0)),
            stage :  stage_num,
            epoch : height,
            start : start_naive_date_time,
            end : end_naive_date_time,
            message,
        })
    }
}
