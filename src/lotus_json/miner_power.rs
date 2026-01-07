// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::actors::miner::MinerPower;
use crate::shim::actors::power::Claim;

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
#[schemars(rename = "MinerPower")]
pub struct MinerPowerLotusJson {
    #[schemars(with = "LotusJson<Claim>")]
    #[serde(with = "crate::lotus_json")]
    miner_power: Claim,
    #[schemars(with = "LotusJson<Claim>")]
    #[serde(with = "crate::lotus_json")]
    total_power: Claim,
    has_min_power: bool,
}

impl HasLotusJson for MinerPower {
    type LotusJson = MinerPowerLotusJson;
    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!({
                "MinerPower": {
                    "RawBytePower": "1",
                    "QualityAdjPower": "2",
                },
                "TotalPower": {
                    "RawBytePower": "3",
                    "QualityAdjPower": "4",
                },
                "HasMinPower": true,
            }),
            Self {
                miner_power: Claim {
                    raw_byte_power: 1.into(),
                    quality_adj_power: 2.into(),
                },
                total_power: Claim {
                    raw_byte_power: 3.into(),
                    quality_adj_power: 4.into(),
                },
                has_min_power: true,
            },
        )]
    }
    fn into_lotus_json(self) -> Self::LotusJson {
        MinerPowerLotusJson {
            miner_power: self.miner_power,
            total_power: self.total_power,
            has_min_power: self.has_min_power,
        }
    }
    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        MinerPower {
            miner_power: lotus_json.miner_power,
            total_power: lotus_json.total_power,
            has_min_power: lotus_json.has_min_power,
        }
    }
}
crate::test_snapshots!(MinerPower);
