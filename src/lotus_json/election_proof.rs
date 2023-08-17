// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocks::{ElectionProof, VRFProof};

use super::*;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ElectionProofLotusJson {
    v_r_f_proof: LotusJson<VRFProof>,
    win_count: LotusJson<i64>,
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

    fn into_lotus_json(self) -> Self::LotusJson {
        let Self {
            win_count,
            vrfproof,
        } = self;
        Self::LotusJson {
            v_r_f_proof: vrfproof.into(),
            win_count: win_count.into(),
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        let Self::LotusJson {
            v_r_f_proof,
            win_count,
        } = lotus_json;
        Self {
            win_count: win_count.into_inner(),
            vrfproof: v_r_f_proof.into_inner(),
        }
    }
}
