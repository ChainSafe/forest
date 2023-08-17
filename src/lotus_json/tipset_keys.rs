// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::blocks::TipsetKeys;
use crate::ipld::FrozenCids;
use ::cid::Cid;

impl HasLotusJson for TipsetKeys {
    type LotusJson = LotusJson<Vec<Cid>>;

    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!([{"/": "baeaaaaa"}]),
            TipsetKeys {
                cids: FrozenCids::from(vec![::cid::Cid::default()]),
            },
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        LotusJson(self.cids.into_iter().collect::<Vec<Cid>>())
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        Self::from(lotus_json.into_inner())
    }
}
