// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::sector::RegisteredSealProof;
use fvm_shared4::sector::RegisteredSealProof as RegisteredSealProofV4;

impl HasLotusJson for RegisteredSealProof {
    type LotusJson = i64;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!(0),
            Self::from(RegisteredSealProofV4::StackedDRG2KiBV1),
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        i64::from(RegisteredSealProofV4::from(self))
    }

    fn from_lotus_json(i: Self::LotusJson) -> Self {
        Self::from(RegisteredSealProofV4::from(i))
    }
}
