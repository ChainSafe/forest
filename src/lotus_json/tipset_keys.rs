// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::blocks::TipsetKey;
use ::cid::Cid;

impl HasLotusJson for TipsetKey {
    type LotusJson = LotusJson<Vec<Cid>>;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!([{"/": "baeaaaaa"}]),
            TipsetKey {
                cids: [::cid::Cid::default()].into_iter().collect(),
            },
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        LotusJson(self.cids.into_iter().collect::<Vec<Cid>>())
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        Self::from_iter(lotus_json.into_inner())
    }
}
