// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::blocks::TipsetKey;
use ::cid::Cid;

impl HasLotusJson for TipsetKey {
    type LotusJson = nunny::Vec<<Cid as HasLotusJson>::LotusJson>;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!([{"/": "baeaaaaa"}]),
            ::nunny::vec![::cid::Cid::default()].into(),
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        self.into_cids()
            .into_iter_ne()
            .map(Cid::into_lotus_json)
            .collect_vec()
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        lotus_json
            .into_iter_ne()
            .map(Cid::from_lotus_json)
            .collect_vec()
            .into()
    }
}
