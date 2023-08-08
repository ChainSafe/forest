// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::sector::RegisteredSealProof;
use fvm_shared3::sector::RegisteredSealProof as RegisteredSealProofV3;

#[derive(Deserialize, Serialize)]
pub struct RegisteredSealProofLotusJson(i64);

impl HasLotusJson for RegisteredSealProof {
    type LotusJson = RegisteredSealProofLotusJson;

    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!(0),
            Self::from(RegisteredSealProofV3::StackedDRG2KiBV1),
        )]
    }
}

impl From<RegisteredSealProofLotusJson> for RegisteredSealProof {
    fn from(RegisteredSealProofLotusJson(value): RegisteredSealProofLotusJson) -> Self {
        Self::from(RegisteredSealProofV3::from(value))
    }
}

impl From<RegisteredSealProof> for RegisteredSealProofLotusJson {
    fn from(value: RegisteredSealProof) -> Self {
        Self(i64::from(RegisteredSealProofV3::from(value)))
    }
}
