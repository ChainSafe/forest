// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::beacon::BeaconEntry;

use super::*;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct BeaconEntryLotusJson {
    round: u64,
    data: VecU8LotusJson,
}

impl HasLotusJson for BeaconEntry {
    type LotusJson = BeaconEntryLotusJson;

    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(json!({"Round": 0, "Data": ""}), BeaconEntry::default())]
    }
}

impl From<BeaconEntry> for BeaconEntryLotusJson {
    fn from(value: BeaconEntry) -> Self {
        let (round, data) = value.into_parts();
        Self {
            round,
            data: data.into(),
        }
    }
}

impl From<BeaconEntryLotusJson> for BeaconEntry {
    fn from(value: BeaconEntryLotusJson) -> Self {
        let BeaconEntryLotusJson { round, data } = value;
        Self::new(round, data.into())
    }
}
