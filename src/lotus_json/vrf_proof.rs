// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocks::VRFProof;

use super::*;

#[derive(Serialize, Deserialize)]
pub struct VRFProofLotusJson(VecU8LotusJson);

impl HasLotusJson for VRFProof {
    type LotusJson = VRFProofLotusJson;

    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!("aGVsbG8gd29ybGQh"),
            VRFProof(Vec::from_iter(*b"hello world!")),
        )]
    }
}

impl From<VRFProofLotusJson> for VRFProof {
    fn from(VRFProofLotusJson(value): VRFProofLotusJson) -> Self {
        Self(value.into())
    }
}

impl From<VRFProof> for VRFProofLotusJson {
    fn from(VRFProof(value): VRFProof) -> Self {
        Self(value.into())
    }
}
