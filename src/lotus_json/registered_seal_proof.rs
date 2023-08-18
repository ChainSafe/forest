// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::sector::RegisteredSealProof;
use fvm_shared3::sector::RegisteredSealProof as RegisteredSealProofV3;

impl HasLotusJson for RegisteredSealProof {
    type LotusJson = i64;

    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!(0),
            Self::from(RegisteredSealProofV3::StackedDRG2KiBV1),
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        i64::from(RegisteredSealProofV3::from(self))
    }

    fn from_lotus_json(i: Self::LotusJson) -> Self {
        Self::from(RegisteredSealProofV3::from(i))
    }
}
