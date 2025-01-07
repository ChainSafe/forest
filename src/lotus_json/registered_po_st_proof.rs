// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::shim::sector::RegisteredPoStProof;
use fvm_shared4::sector::RegisteredPoStProof as RegisteredPoStProofV4;

use super::*;

impl HasLotusJson for RegisteredPoStProof {
    type LotusJson = i64;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!(0),
            RegisteredPoStProof::from(RegisteredPoStProofV4::StackedDRGWinning2KiBV1),
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        i64::from(RegisteredPoStProofV4::from(self))
    }

    fn from_lotus_json(i: Self::LotusJson) -> Self {
        Self::from(RegisteredPoStProofV4::from(i))
    }
}
