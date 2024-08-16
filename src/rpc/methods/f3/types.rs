// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::lotus_json::lotus_json_with_self;
use fvm_shared4::ActorID;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// TipSetKey is the canonically ordered concatenation of the block CIDs in a tipset.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct F3TipSetKey(Vec<u8>);
lotus_json_with_self!(F3TipSetKey);

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct F3TipSet {
    key: F3TipSetKey,
    /// The verifiable oracle randomness used to elect this block's author leader
    beacon: Vec<u8>,
    /// The period in which a new block is generated.
    /// There may be multiple rounds in an epoch.
    epoch: ChainEpoch,
    /// Block creation time, in seconds since the Unix epoch
    timestamp: u64,
}
lotus_json_with_self!(F3TipSet);

/// PowerEntry represents a single entry in the PowerTable, including ActorID and its StoragePower and PubKey.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct F3PowerEntry {
    id: ActorID,
    #[schemars(with = "String")]
    power: num::BigInt,
    pub_key: Vec<u8>,
}
lotus_json_with_self!(F3PowerEntry);
