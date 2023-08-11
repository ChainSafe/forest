// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::sector::{RegisteredSealProof, SectorInfo};
use ::cid::Cid;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SectorInfoLotusJson {
    seal_proof: LotusJson<RegisteredSealProof>,
    sector_number: LotusJson<u64>,
    sealed_c_i_d: LotusJson<Cid>,
}

impl HasLotusJson for SectorInfo {
    type LotusJson = SectorInfoLotusJson;

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
                fvm_shared3::sector::RegisteredSealProof::StackedDRG2KiBV1,
                0,
                ::cid::Cid::default(),
            ),
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        let fvm_shared3::sector::SectorInfo {
            proof,
            sector_number,
            sealed_cid,
        } = From::from(self);
        Self::LotusJson {
            seal_proof: crate::shim::sector::RegisteredSealProof::from(proof).into(),
            sector_number: sector_number.into(),
            sealed_c_i_d: sealed_cid.into(),
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        let Self::LotusJson {
            seal_proof,
            sector_number,
            sealed_c_i_d,
        } = lotus_json;
        Self::new(
            seal_proof.into_inner().into(),
            sector_number.into_inner(),
            sealed_c_i_d.into_inner(),
        )
    }
}
