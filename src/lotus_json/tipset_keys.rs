// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::blocks::TipsetKey;
use ::cid::Cid;
use ::nonempty::NonEmpty;

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct TipsetKeyLotusJson(
    #[schemars(with = "LotusJson<Vec<Cid>>")]
    #[serde(with = "crate::lotus_json")]
    NonEmpty<Cid>,
);

impl HasLotusJson for TipsetKey {
    type LotusJson = TipsetKeyLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!([{"/": "baeaaaaa"}]),
            ::nonempty::nonempty![::cid::Cid::default()].into(),
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        TipsetKeyLotusJson(self.into_cids())
    }

    fn from_lotus_json(TipsetKeyLotusJson(lotus_json): Self::LotusJson) -> Self {
        Self::from(lotus_json)
    }
}
