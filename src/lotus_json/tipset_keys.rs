// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::blocks::TipsetKey;
use crate::cid_collections::SmallCidNonEmptyVec;
use ::cid::Cid;
use ::nonempty::{nonempty, NonEmpty};

impl HasLotusJson for TipsetKey {
    type LotusJson = LotusJson<NonEmpty<Cid>>;

    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!([{"/": "baeaaaaa"}]),
            TipsetKey {
                cids: SmallCidNonEmptyVec::from(nonempty![::cid::Cid::default()]),
            },
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        LotusJson(self.cids.into_cids())
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        Self::from(lotus_json.into_inner())
    }
}
