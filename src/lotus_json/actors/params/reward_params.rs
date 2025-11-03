// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::address::Address;
use crate::shim::econ::TokenAmount;
use num_bigint::BigInt;
use pastey::paste;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(transparent)]
pub struct RewardConstructorParamsLotusJson(
    #[schemars(with = "LotusJson<Option<BigInt>>")]
    #[serde(with = "crate::lotus_json")]
    Option<BigInt>,
);

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct AwardBlockRewardParamsLotusJson {
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub miner: Address,
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub penalty: TokenAmount,
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub gas_reward: TokenAmount,
    pub win_count: i64,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(transparent)]
pub struct UpdateNetworkKPIParamsLotusJson(
    #[schemars(with = "LotusJson<Option<BigInt>>")]
    #[serde(with = "crate::lotus_json")]
    Option<BigInt>,
);

// Implementation for ConstructorParams
macro_rules! impl_reward_constructor_params {
    ($type_suffix:path: $($version:literal),+) => {
        $(
            paste! {
                impl HasLotusJson for fil_actor_reward_state::[<v $version>]::ConstructorParams {
                    type LotusJson = RewardConstructorParamsLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![
                            (
                                json!(null),
                                Self {
                                    power: None,
                                },
                            ),
                            (
                                json!("1000"),
                                Self {
                                    power: Some($type_suffix::bigint_ser::BigIntDe(BigInt::from(1000))),
                                },
                            ),
                        ]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        RewardConstructorParamsLotusJson(self.power.map(|p| p.0))
                    }

                    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                        Self {
                            power: lotus_json.0.map(|p| $type_suffix::bigint_ser::BigIntDe(p)),
                        }
                    }
                }
            }
        )+
    };
}

// Implementation for AwardBlockRewardParams
macro_rules! impl_award_block_reward_params {
    ($($version:literal),+) => {
        $(
            paste! {
                impl HasLotusJson for fil_actor_reward_state::[<v $version>]::AwardBlockRewardParams {
                    type LotusJson = AwardBlockRewardParamsLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![
                            (
                                json!({
                                    "Miner": "f01234",
                                    "Penalty": "0",
                                    "GasReward": "1000",
                                    "WinCount": 1
                                }),
                                Self {
                                    miner: Address::new_id(1234).into(),
                                    penalty: TokenAmount::from_atto(0).into(),
                                    gas_reward: TokenAmount::from_atto(1000).into(),
                                    win_count: 1,
                                },
                            ),
                        ]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        AwardBlockRewardParamsLotusJson {
                            miner: self.miner.into(),
                            penalty: self.penalty.into(),
                            gas_reward: self.gas_reward.into(),
                            win_count: self.win_count,
                        }
                    }

                    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                        Self {
                            miner: lotus_json.miner.into(),
                            penalty: TokenAmount::from(lotus_json.penalty).into(),
                            gas_reward: TokenAmount::from(lotus_json.gas_reward).into(),
                            win_count: lotus_json.win_count,
                        }
                    }
                }
            }
        )+
    };
}

// Implementation for UpdateNetworkKPIParams
macro_rules! impl_update_network_kpi_params {
    ($type_suffix:path: $($version:literal),+) => {
        $(
            paste! {
                impl HasLotusJson for fil_actor_reward_state::[<v $version>]::UpdateNetworkKPIParams {
                    type LotusJson = UpdateNetworkKPIParamsLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![
                            (
                                json!(null),
                                Self {
                                    curr_realized_power: None,
                                },
                            ),
                            (
                                json!("2000"),
                                Self {
                                    curr_realized_power: Some($type_suffix::bigint_ser::BigIntDe(BigInt::from(2000))),
                                },
                            ),
                        ]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        UpdateNetworkKPIParamsLotusJson(self.curr_realized_power.map(|p| p.0))
                    }

                    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                        Self {
                            curr_realized_power: lotus_json.0.map(|p| $type_suffix::bigint_ser::BigIntDe(p)),
                        }
                    }
                }
            }
        )+
    };
}

impl_reward_constructor_params!(fvm_shared4::bigint: 17, 16, 15, 14, 13, 12);
impl_reward_constructor_params!(fvm_shared3::bigint: 11);
impl_award_block_reward_params!(17, 16, 15, 14, 13, 12, 11, 10, 9, 8);
impl_update_network_kpi_params!(fvm_shared4::bigint: 17, 16, 15, 14, 13, 12);
impl_update_network_kpi_params!(fvm_shared3::bigint: 11);
