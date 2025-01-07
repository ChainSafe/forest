// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use super::*;

use crate::chain_sync::SyncStage;

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "SyncStage")]
pub struct SyncStageLotusJson(
    #[schemars(with = "String")]
    #[serde(with = "crate::lotus_json::stringify")]
    SyncStage,
);

impl HasLotusJson for SyncStage {
    type LotusJson = SyncStageLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(json!("idle worker"), Self::Idle)]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        SyncStageLotusJson(self)
    }

    fn from_lotus_json(SyncStageLotusJson(sync_stage): Self::LotusJson) -> Self {
        sync_stage
    }
}
