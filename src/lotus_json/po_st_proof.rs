// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::shim::sector::{PoStProof, RegisteredPoStProof};
use fvm_shared4::sector::PoStProof as PoStProofV4;

use super::*;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
#[schemars(rename = "PoStProof")]
pub struct PoStProofLotusJson {
    #[schemars(with = "LotusJson<RegisteredPoStProof>")]
    #[serde(with = "crate::lotus_json")]
    po_st_proof: RegisteredPoStProof,
    #[schemars(with = "LotusJson<Vec<u8>>")]
    #[serde(with = "crate::lotus_json")]
    proof_bytes: Vec<u8>,
}

impl HasLotusJson for PoStProof {
    type LotusJson = PoStProofLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!({
                "PoStProof": 0,
                "ProofBytes": "aGVsbG8gd29ybGQh"
            }),
            PoStProof::new(
                crate::shim::sector::RegisteredPoStProof::from(
                    crate::shim::sector::RegisteredPoStProofV4::StackedDRGWinning2KiBV1,
                ),
                Vec::from_iter(*b"hello world!"),
            ),
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        let PoStProofV4 {
            post_proof,
            proof_bytes,
        } = self.into();
        Self::LotusJson {
            po_st_proof: post_proof.into(),
            proof_bytes,
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        let Self::LotusJson {
            po_st_proof,
            proof_bytes,
        } = lotus_json;
        Self::from(PoStProofV4 {
            post_proof: po_st_proof.into(),
            proof_bytes,
        })
    }
}
