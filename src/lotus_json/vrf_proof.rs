// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::blocks::VRFProof;

impl HasLotusJson for VRFProof {
    type LotusJson = <Vec<u8> as HasLotusJson>::LotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!("aGVsbG8gd29ybGQh"),
            VRFProof(Vec::from_iter(*b"hello world!")),
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        let Self(vec) = self;
        vec.into_lotus_json()
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        Self(HasLotusJson::from_lotus_json(lotus_json))
    }
}
