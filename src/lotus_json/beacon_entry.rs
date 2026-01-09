// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::beacon::BeaconEntry;

use super::*;

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
#[schemars(rename = "BeaconEntry")]
pub struct BeaconEntryLotusJson {
    round: u64,
    #[schemars(with = "LotusJson<Vec<u8>>")]
    #[serde(with = "crate::lotus_json")]
    data: Vec<u8>,
}

impl HasLotusJson for BeaconEntry {
    type LotusJson = BeaconEntryLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(json!({"Round": 0, "Data": null}), BeaconEntry::default())]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        let (round, data) = self.into_parts();
        Self::LotusJson { round, data }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        let Self::LotusJson { round, data } = lotus_json;
        Self::new(round, data)
    }
}
