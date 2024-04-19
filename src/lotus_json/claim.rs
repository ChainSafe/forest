// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fil_actor_interface::power::Claim;

use super::*;

#[derive(Default, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct ClaimLotusJson {
    /// Sum of raw byte power for a miner's sectors.
    pub raw_byte_power: LotusJson<num::BigInt>,
    /// Sum of quality adjusted power for a miner's sectors.
    pub quality_adj_power: LotusJson<num::BigInt>,
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
            raw_byte_power: LotusJson(self.raw_byte_power),
            quality_adj_power: LotusJson(self.quality_adj_power),
        }
    }
    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        Claim {
            raw_byte_power: lotus_json.raw_byte_power.into_inner(),
            quality_adj_power: lotus_json.quality_adj_power.into_inner(),
        }
    }
}

#[test]
fn snapshots() {
    assert_all_snapshots::<Claim>();
}
