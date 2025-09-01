// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::piece::PaddedPieceSize;

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "PaddedPieceSize")]
pub struct PaddedPieceSizeLotusJson(#[schemars(with = "u64")] PaddedPieceSize);

impl HasLotusJson for PaddedPieceSize {
    type LotusJson = PaddedPieceSizeLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        PaddedPieceSizeLotusJson(self)
    }

    fn from_lotus_json(PaddedPieceSizeLotusJson(inner): Self::LotusJson) -> Self {
        inner
    }
}
