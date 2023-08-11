// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::crypto::SignatureType;

impl HasLotusJson for SignatureType {
    type LotusJson = Self; // serialization happens to be the same here

    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(json!(2), SignatureType::Bls)]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        self
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        lotus_json
    }
}
