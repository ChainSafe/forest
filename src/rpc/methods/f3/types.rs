// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::{
    blocks::{Tipset, TipsetKey},
    lotus_json::{base64_standard, lotus_json_with_self},
};
use cid::{multihash::MultihashDigest as _, Cid};
use fvm_shared4::ActorID;
use itertools::Itertools as _;
use once_cell::sync::Lazy;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

/// TipSetKey is the canonically ordered concatenation of the block CIDs in a tipset.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct F3TipSetKey(
    #[schemars(with = "String")]
    #[serde(with = "base64_standard")]
    pub Vec<u8>,
);
lotus_json_with_self!(F3TipSetKey);

impl From<&TipsetKey> for F3TipSetKey {
    fn from(tsk: &TipsetKey) -> Self {
        let bytes = tsk.iter().flat_map(|cid| cid.to_bytes()).collect();
        Self(bytes)
    }
}

impl From<TipsetKey> for F3TipSetKey {
    fn from(tsk: TipsetKey) -> Self {
        (&tsk).into()
    }
}

impl TryFrom<F3TipSetKey> for TipsetKey {
    type Error = anyhow::Error;

    fn try_from(tsk: F3TipSetKey) -> Result<Self, Self::Error> {
        static BLOCK_HEADER_CID_LEN: Lazy<usize> = Lazy::new(|| {
            let buf = [0_u8; 256];
            let cid = Cid::new_v1(
                fvm_ipld_encoding::DAG_CBOR,
                cid::multihash::Code::Blake2b256.digest(&buf),
            );
            cid.to_bytes().len()
        });

        let cids: Vec<Cid> = tsk
            .0
            .chunks(*BLOCK_HEADER_CID_LEN)
            .map(Cid::read_bytes)
            .try_collect()?;

        Ok(nunny::Vec::new(cids)
            .map_err(|_| anyhow::anyhow!("tipset key cannot be empty"))?
            .into())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct F3TipSet {
    pub key: F3TipSetKey,
    /// The verifiable oracle randomness used to elect this block's author leader
    #[schemars(with = "String")]
    #[serde(with = "base64_standard")]
    pub beacon: Vec<u8>,
    /// The period in which a new block is generated.
    /// There may be multiple rounds in an epoch.
    pub epoch: ChainEpoch,
    /// Block creation time, in seconds since the Unix epoch
    pub timestamp: u64,
}
lotus_json_with_self!(F3TipSet);

impl From<Tipset> for F3TipSet {
    fn from(ts: Tipset) -> Self {
        let key = ts.key().into();
        let beacon = {
            let entries = &ts.block_headers().first().beacon_entries;
            if let Some(last) = entries.last() {
                last.signature().to_vec()
            } else {
                vec![0; 32]
            }
        };
        let epoch = ts.epoch();
        let timestamp = ts.block_headers().first().timestamp;
        Self {
            key,
            beacon,
            epoch,
            timestamp,
        }
    }
}

impl From<Arc<Tipset>> for F3TipSet {
    fn from(ts: Arc<Tipset>) -> Self {
        Arc::unwrap_or_clone(ts).into()
    }
}

/// PowerEntry represents a single entry in the PowerTable, including ActorID and its StoragePower and PubKey.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Eq, PartialEq)]
pub struct F3PowerEntry {
    pub id: ActorID,
    #[schemars(with = "String")]
    pub power: num::BigInt,
    #[schemars(with = "String")]
    #[serde(with = "base64_standard")]
    pub pub_key: Vec<u8>,
}
lotus_json_with_self!(F3PowerEntry);

/// Entries are sorted descending order of their power, where entries with equal power are
/// sorted by ascending order of their ID.
/// This ordering is guaranteed to be stable, since a valid PowerTable cannot contain entries with duplicate IDs
impl Ord for F3PowerEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        match other.power.cmp(&self.power) {
            Ordering::Equal => self.id.cmp(&other.id),
            ord => ord,
        }
    }
}

impl PartialOrd for F3PowerEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
