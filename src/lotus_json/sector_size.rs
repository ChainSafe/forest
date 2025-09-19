// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::sector::SectorSize;

#[derive(Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "SectorSize")]
// This should probably be a JSON Schema enum
pub struct SectorSizeLotusJson(#[schemars(with = "u64")] SectorSize);

impl HasLotusJson for SectorSize {
    type LotusJson = SectorSizeLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(json!(2048), Self::_2KiB)]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        SectorSizeLotusJson(self)
    }

    fn from_lotus_json(SectorSizeLotusJson(inner): Self::LotusJson) -> Self {
        inner
    }
}
