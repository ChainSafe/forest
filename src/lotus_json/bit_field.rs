// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

use fil_actors_shared::fvm_ipld_bitfield::{json::BitFieldJson, BitField};

#[derive(Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "BitField")]
pub struct BitFieldLotusJson(#[schemars(with = "Option<Vec<u8>>")] pub BitFieldJson);

impl Clone for BitFieldLotusJson {
    fn clone(&self) -> Self {
        Self(BitFieldJson(self.0 .0.clone()))
    }
}

impl HasLotusJson for BitField {
    type LotusJson = BitFieldLotusJson;
    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![
            (json!([0]), Self::new()),
            (json!([1, 1]), {
                let mut it = Self::new();
                it.set(1);
                it
            }),
        ]
    }
    fn into_lotus_json(self) -> Self::LotusJson {
        BitFieldLotusJson(BitFieldJson(self))
    }
    fn from_lotus_json(BitFieldLotusJson(BitFieldJson(it)): Self::LotusJson) -> Self {
        it
    }
}

#[test]
fn snapshots() {
    assert_all_snapshots::<BitField>();
}
