// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocks::ElectionProof;

use super::*;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ElectionProofLotusJson {
    v_r_f_proof: VRFProofLotusJson,
    win_count: i64,
}

impl HasLotusJson for ElectionProof {
    type LotusJson = ElectionProofLotusJson;

    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!({
                "WinCount": 0,
                "VRFProof": ""
            }),
            ElectionProof::default(),
        )]
    }
}

impl From<ElectionProofLotusJson> for ElectionProof {
    fn from(value: ElectionProofLotusJson) -> Self {
        let ElectionProofLotusJson {
            v_r_f_proof: vrfproof,
            win_count,
        } = value;
        Self {
            win_count,
            vrfproof: vrfproof.into(),
        }
    }
}

impl From<ElectionProof> for ElectionProofLotusJson {
    fn from(value: ElectionProof) -> Self {
        let ElectionProof {
            win_count,
            vrfproof,
        } = value;
        Self {
            v_r_f_proof: vrfproof.into(),
            win_count,
        }
    }
}
