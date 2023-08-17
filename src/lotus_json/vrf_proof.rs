// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocks::VRFProof;

use super::*;

impl HasLotusJson for VRFProof {
    type LotusJson = LotusJson<Vec<u8>>;

    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!("aGVsbG8gd29ybGQh"),
            VRFProof(Vec::from_iter(*b"hello world!")),
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        let Self(vec) = self;
        vec.into()
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        Self(lotus_json.into_inner())
    }
}
