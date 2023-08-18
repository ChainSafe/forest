// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::beacon::BeaconEntry;

use super::*;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct BeaconEntryLotusJson {
    round: LotusJson<u64>,
    data: LotusJson<Vec<u8>>,
}

impl HasLotusJson for BeaconEntry {
    type LotusJson = BeaconEntryLotusJson;

    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(json!({"Round": 0, "Data": ""}), BeaconEntry::default())]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        let (round, data) = self.into_parts();
        Self::LotusJson {
            round: round.into(),
            data: data.into(),
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        let Self::LotusJson { round, data } = lotus_json;
        Self::new(round.into_inner(), data.into_inner())
    }
}
