// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::shim::sector::RegisteredPoStProof;
use fvm_shared3::sector::RegisteredPoStProof as RegisteredPoStProofV3;

use super::*;

#[derive(Serialize, Deserialize)]
pub struct RegisteredPoStProofLotusJson(i64);

impl HasLotusJson for RegisteredPoStProof {
    type LotusJson = RegisteredPoStProofLotusJson;

    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!(0),
            RegisteredPoStProof::from(RegisteredPoStProofV3::StackedDRGWinning2KiBV1),
        )]
    }
}

impl From<RegisteredPoStProof> for RegisteredPoStProofLotusJson {
    fn from(value: RegisteredPoStProof) -> Self {
        Self(i64::from(RegisteredPoStProofV3::from(value)))
    }
}

impl From<RegisteredPoStProofLotusJson> for RegisteredPoStProof {
    fn from(value: RegisteredPoStProofLotusJson) -> Self {
        Self::from(RegisteredPoStProofV3::from(value.0))
    }
}
