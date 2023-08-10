// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::shim::sector::PoStProof;
use fvm_shared3::sector::PoStProof as PoStProofV3;

use super::*;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PoStProofLotusJson {
    po_st_proof: RegisteredPoStProofLotusJson,
    proof_bytes: VecU8LotusJson,
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
}

impl From<PoStProof> for PoStProofLotusJson {
    fn from(value: PoStProof) -> Self {
        let PoStProofV3 {
            post_proof,
            proof_bytes,
        } = value.into();
        Self {
            po_st_proof: crate::shim::sector::RegisteredPoStProof::from(post_proof).into(),
            proof_bytes: proof_bytes.into(),
        }
    }
}

impl From<PoStProofLotusJson> for PoStProof {
    fn from(value: PoStProofLotusJson) -> Self {
        let PoStProofLotusJson {
            po_st_proof,
            proof_bytes,
        } = value;
        Self::from(PoStProofV3 {
            post_proof: crate::shim::sector::RegisteredPoStProof::from(po_st_proof).into(),
            proof_bytes: proof_bytes.into(),
        })
    }
}
