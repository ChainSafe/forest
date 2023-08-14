// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocks::TipsetKeys;
use crate::ipld::FrozenCids;
use ::cid::Cid;

use super::*;

#[derive(Serialize, Deserialize)]
pub struct TipsetKeysLotusJson(VecLotusJson<CidLotusJson>);

impl HasLotusJson for TipsetKeys {
    type LotusJson = TipsetKeysLotusJson;

    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!([{"/": "baeaaaaa"}]),
            TipsetKeys {
                cids: FrozenCids::from(vec![::cid::Cid::default()]),
            },
        )]
    }
}

impl From<TipsetKeys> for TipsetKeysLotusJson {
    fn from(value: TipsetKeys) -> Self {
        let TipsetKeys { cids } = value;
        Self(VecLotusJson::<CidLotusJson>::from(Vec::<Cid>::from(cids)))
    }
}

impl From<TipsetKeysLotusJson> for TipsetKeys {
    fn from(value: TipsetKeysLotusJson) -> Self {
        let TipsetKeysLotusJson(cids) = value;
        Self { cids: FrozenCids::from(Vec::<Cid>::from(cids)) }
    }
}
