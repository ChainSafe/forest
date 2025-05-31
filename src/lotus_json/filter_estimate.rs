// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use super::*;
use fil_actors_shared::v16::reward::FilterEstimate;
use num::BigInt;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct FilterEstimateLotusJson {
    #[schemars(with = "LotusJson<BigInt>")]
    #[serde(with = "crate::lotus_json")]
    pub position_estimate: BigInt,
    #[schemars(with = "LotusJson<BigInt>")]
    #[serde(with = "crate::lotus_json")]
    pub velocity_estimate: BigInt,
}

// Only implementing for V16 FilterEstimate because all the versions have the
// same internal fields type (BigInt).
impl HasLotusJson for FilterEstimate {
    type LotusJson = FilterEstimateLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!({
                "Position": "0",
                "Velocity" : "0",
            }),
            FilterEstimate::default(),
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        FilterEstimateLotusJson {
            position_estimate: self.position,
            velocity_estimate: self.velocity,
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        FilterEstimate {
            position: lotus_json.position_estimate,
            velocity: lotus_json.velocity_estimate,
        }
    }
}
