// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::sector::{RegisteredSealProof, SectorInfo};
use ::cid::Cid;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
#[schemars(rename = "SectorInfo")]
pub struct SectorInfoLotusJson {
    #[schemars(with = "LotusJson<RegisteredSealProof>")]
    #[serde(with = "crate::lotus_json")]
    seal_proof: RegisteredSealProof,
    sector_number: u64,
    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    sealed_c_i_d: Cid,
}

impl HasLotusJson for SectorInfo {
    type LotusJson = SectorInfoLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!({
                "SealProof": 0,
                "SectorNumber": 0,
                "SealedCID": {
                    "/": "baeaaaaa"
                }
            }),
            Self::new(
                fvm_shared4::sector::RegisteredSealProof::StackedDRG2KiBV1,
                0,
                ::cid::Cid::default(),
            ),
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        let fvm_shared4::sector::SectorInfo {
            proof,
            sector_number,
            sealed_cid,
        } = From::from(self);
        Self::LotusJson {
            seal_proof: crate::shim::sector::RegisteredSealProof::from(proof),
            sector_number,
            sealed_c_i_d: sealed_cid,
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        let Self::LotusJson {
            seal_proof,
            sector_number,
            sealed_c_i_d,
        } = lotus_json;
        Self::new(seal_proof.into(), sector_number, sealed_c_i_d)
    }
}
