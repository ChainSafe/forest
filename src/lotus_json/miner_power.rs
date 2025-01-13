// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::actors::miner::MinerPower;
use crate::shim::actors::power::Claim;

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
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
        unimplemented!("see commented-out test, below")
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

// MinerPower: !Debug
// #[test]
// fn snapshots() {
//     assert_all_snapshots::<MinerPower>();
// }
