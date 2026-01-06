// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use num::BigInt;
use pastey::paste;

#[derive(Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct FilterEstimateLotusJson {
    #[schemars(with = "LotusJson<BigInt>")]
    #[serde(with = "crate::lotus_json")]
    pub position_estimate: BigInt,
    #[schemars(with = "LotusJson<BigInt>")]
    #[serde(with = "crate::lotus_json")]
    pub velocity_estimate: BigInt,
}

// Macro to implement HasLotusJson for FilterEstimate across all versions
macro_rules! impl_filter_estimate_lotus_json {
    ($($version:literal),+) => {
        $(
        paste! {
            mod [<impl_filter_estimate_lotus_json_ $version>] {
                use super::*;
                type T = fil_actors_shared::[<v $version>]::reward::FilterEstimate;
                #[test]
                fn snapshots() {
                    crate::lotus_json::assert_all_snapshots::<T>();
                }
                impl HasLotusJson for T {
                    type LotusJson = FilterEstimateLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![(
                            json!({
                                "PositionEstimate": "0",
                                "VelocityEstimate": "0",
                            }),
                            Self::default(),
                        )]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        FilterEstimateLotusJson {
                            position_estimate: self.position,
                            velocity_estimate: self.velocity,
                        }
                    }

                    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                        Self {
                            position: lotus_json.position_estimate,
                            velocity: lotus_json.velocity_estimate,
                        }
                    }
                }
            }
        }
        )+
    };
}

// Implement HasLotusJson for FilterEstimate for all actor versions
impl_filter_estimate_lotus_json!(14, 15, 16, 17);
