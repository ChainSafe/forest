// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::shim::actors::power::Claim;

use super::*;

#[derive(Default, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
#[schemars(rename = "Claim")]
pub struct ClaimLotusJson {
    #[schemars(with = "LotusJson<num::BigInt>")]
    #[serde(with = "crate::lotus_json")]
    /// Sum of raw byte power for a miner's sectors.
    pub raw_byte_power: num::BigInt,
    #[schemars(with = "LotusJson<num::BigInt>")]
    #[serde(with = "crate::lotus_json")]
    /// Sum of quality adjusted power for a miner's sectors.
    pub quality_adj_power: num::BigInt,
}

impl HasLotusJson for Claim {
    type LotusJson = ClaimLotusJson;
    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        use num::{BigInt, Zero};

        vec![(
            json! {{
                "RawBytePower": "0",
                "QualityAdjPower": "0",
            }},
            Claim {
                raw_byte_power: BigInt::zero(),
                quality_adj_power: BigInt::zero(),
            },
        )]
    }
    fn into_lotus_json(self) -> Self::LotusJson {
        ClaimLotusJson {
            raw_byte_power: self.raw_byte_power,
            quality_adj_power: self.quality_adj_power,
        }
    }
    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        Claim {
            raw_byte_power: lotus_json.raw_byte_power,
            quality_adj_power: lotus_json.quality_adj_power,
        }
    }
}

#[test]
fn snapshots() {
    assert_all_snapshots::<Claim>();
}
