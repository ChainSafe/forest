// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::crypto::SignatureType;

#[derive(Deserialize, Serialize, From, Into)]
// serialization happens to be the same here
pub struct SignatureTypeLotusJson(SignatureType);

impl HasLotusJson for SignatureType {
    type LotusJson = SignatureTypeLotusJson;

    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(json!(2), SignatureType::Bls)]
    }
}
