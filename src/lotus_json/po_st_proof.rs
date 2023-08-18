// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::shim::sector::{PoStProof, RegisteredPoStProof};
use fvm_shared3::sector::PoStProof as PoStProofV3;

use super::*;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PoStProofLotusJson {
    po_st_proof: LotusJson<RegisteredPoStProof>,
    proof_bytes: LotusJson<Vec<u8>>,
}

impl HasLotusJson for PoStProof {
    type LotusJson = PoStProofLotusJson;

    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!({
                "PoStProof": 0,
                "ProofBytes": "aGVsbG8gd29ybGQh"
            }),
            PoStProof::new(
                crate::shim::sector::RegisteredPoStProof::from(
                    crate::shim::sector::RegisteredPoStProofV3::StackedDRGWinning2KiBV1,
                ),
                Vec::from_iter(*b"hello world!"),
            ),
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        let PoStProofV3 {
            post_proof,
            proof_bytes,
        } = self.into();
        Self::LotusJson {
            po_st_proof: crate::shim::sector::RegisteredPoStProof::from(post_proof).into(),
            proof_bytes: proof_bytes.into(),
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        let Self::LotusJson {
            po_st_proof,
            proof_bytes,
        } = lotus_json;
        Self::from(PoStProofV3 {
            post_proof: po_st_proof.into_inner().into(),
            proof_bytes: proof_bytes.into_inner(),
        })
    }
}
