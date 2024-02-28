// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::blocks::TipsetKey;
use ::cid::Cid;
use ::nonempty::NonEmpty;

impl HasLotusJson for TipsetKey {
    type LotusJson = LotusJson<NonEmpty<Cid>>;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!([{"/": "baeaaaaa"}]),
            ::nonempty::nonempty![::cid::Cid::default()].into(),
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        LotusJson(self.into_cids())
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        Self::from(lotus_json.into_inner())
    }
}
