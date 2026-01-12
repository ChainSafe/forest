// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::sector::{ExtendedSectorInfo, RegisteredSealProof};
use ::cid::Cid;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
#[schemars(rename = "ExtendedSectorInfo")]
pub struct ExtendedSectorInfoLotusJson {
    #[schemars(with = "LotusJson<RegisteredSealProof>")]
    #[serde(with = "crate::lotus_json")]
    seal_proof: RegisteredSealProof,
    sector_number: u64,
    #[schemars(with = "LotusJson<Option<Cid>>")]
    #[serde(with = "crate::lotus_json")]
    sector_key: Option<Cid>,
    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    sealed_c_i_d: Cid,
}

impl HasLotusJson for ExtendedSectorInfo {
    type LotusJson = ExtendedSectorInfoLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!({
                "SealProof": 0,
                "SectorNumber": 0,
                "SectorKey": null,
                "SealedCID": {
                    "/": "baeaaaaa"
                }
            }),
            Self {
                proof: fvm_shared3::sector::RegisteredSealProof::StackedDRG2KiBV1.into(),
                sector_number: 0,
                sector_key: None,
                sealed_cid: ::cid::Cid::default(),
            },
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        Self::LotusJson {
            seal_proof: self.proof,
            sector_number: self.sector_number,
            sector_key: self.sector_key,
            sealed_c_i_d: self.sealed_cid,
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        let Self::LotusJson {
            seal_proof,
            sector_number,
            sector_key,
            sealed_c_i_d,
        } = lotus_json;
        Self {
            proof: seal_proof,
            sector_number,
            sector_key,
            sealed_cid: sealed_c_i_d,
        }
    }
}
