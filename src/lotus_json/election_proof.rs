// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocks::{ElectionProof, VRFProof};

use super::*;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
#[schemars(rename = "ElectionProof")]
pub struct ElectionProofLotusJson {
    #[schemars(with = "LotusJson<VRFProof>")]
    #[serde(with = "crate::lotus_json")]
    v_r_f_proof: VRFProof,
    win_count: i64,
}

impl HasLotusJson for ElectionProof {
    type LotusJson = ElectionProofLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!({
                "WinCount": 0,
                "VRFProof": null
            }),
            ElectionProof::default(),
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        let Self {
            win_count,
            vrfproof,
        } = self;
        Self::LotusJson {
            v_r_f_proof: vrfproof,
            win_count,
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        let Self::LotusJson {
            v_r_f_proof,
            win_count,
        } = lotus_json;
        Self {
            win_count,
            vrfproof: v_r_f_proof,
        }
    }
}
