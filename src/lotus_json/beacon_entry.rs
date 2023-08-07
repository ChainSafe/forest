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

#[cfg(test)]
quickcheck! {
    fn round_trip(val: BeaconEntry) -> bool {
        assert_via_json(val);
        true
    }
}
