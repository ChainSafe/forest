// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::shim::sector::RegisteredPoStProof;
use fvm_shared3::sector::RegisteredPoStProof as RegisteredPoStProofV3;

use super::*;

impl HasLotusJson for RegisteredPoStProof {
    type LotusJson = i64;

    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!(0),
            RegisteredPoStProof::from(RegisteredPoStProofV3::StackedDRGWinning2KiBV1),
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        i64::from(RegisteredPoStProofV3::from(self))
    }

    fn from_lotus_json(i: Self::LotusJson) -> Self {
        Self::from(RegisteredPoStProofV3::from(i))
    }
}
