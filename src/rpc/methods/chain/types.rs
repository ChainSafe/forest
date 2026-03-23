// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, Eq, PartialEq, Default)]
#[serde(rename_all = "PascalCase")]
pub struct ObjStat {
    pub size: usize,
    pub links: usize,
}
lotus_json_with_self!(ObjStat);

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, Eq, PartialEq, Default)]
#[serde(rename_all = "PascalCase")]
pub struct ChainIndexValidation {
    /// the key of the canonical tipset for this epoch
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<ApiTipsetKey>")]
    pub tip_set_key: ApiTipsetKey,
    /// the epoch height at which the validation is performed.
    pub height: ChainEpoch,
    /// the number of indexed messages for the canonical tipset at this epoch
    pub indexed_messages_count: u64,
    /// the number of indexed events for the canonical tipset at this epoch
    pub indexed_events_count: u64,
    /// the number of indexed event entries for the canonical tipset at this epoch
    pub indexed_event_entries_count: u64,
    /// whether missing data was successfully backfilled into the index during validation
    pub backfilled: bool,
    /// if the epoch corresponds to a null round and therefore does not have any indexed messages or events
    pub is_null_round: bool,
}
lotus_json_with_self!(ChainIndexValidation);
