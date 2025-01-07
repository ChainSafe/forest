// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::rpc::types::EventEntry;

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, Eq, PartialEq, Default)]
#[serde(rename_all = "PascalCase")]
pub struct ObjStat {
    pub size: usize,
    pub links: usize,
}
lotus_json_with_self!(ObjStat);

#[derive(Serialize, Deserialize, JsonSchema, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct Event {
    /// Actor ID
    pub emitter: u64,
    pub entries: Vec<EventEntry>,
}
lotus_json_with_self!(Event);
