// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocks::TipsetKeys;

use super::*;

#[derive(Serialize, Deserialize)]
pub struct TipsetKeysLotusJson(VecLotusJson<CidLotusJson>);

impl HasLotusJson for TipsetKeys {
    type LotusJson = TipsetKeysLotusJson;

    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!([{"/": "baeaaaaa"}]),
            TipsetKeys {
                cids: vec![::cid::Cid::default()],
            },
        )]
    }
}

impl From<TipsetKeys> for TipsetKeysLotusJson {
    fn from(value: TipsetKeys) -> Self {
        let TipsetKeys { cids } = value;
        Self(cids.into())
    }
}

impl From<TipsetKeysLotusJson> for TipsetKeys {
    fn from(value: TipsetKeysLotusJson) -> Self {
        let TipsetKeysLotusJson(cids) = value;
        Self { cids: cids.into() }
    }
}
