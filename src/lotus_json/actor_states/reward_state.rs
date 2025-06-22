// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::actors::reward::State;
use crate::shim::clock::ChainEpoch;
use crate::shim::econ::TokenAmount;
use fil_actors_shared::v16::reward::FilterEstimate;
use num_bigint::BigInt;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
#[schemars(rename = "RewardState")]
pub struct RewardStateLotusJson {
    #[schemars(with = "LotusJson<BigInt>")]
    #[serde(with = "crate::lotus_json")]
    pub cumsum_baseline: BigInt,

    #[schemars(with = "LotusJson<BigInt>")]
    #[serde(with = "crate::lotus_json")]
    pub cumsum_realized: BigInt,

    #[schemars(with = "LotusJson<ChainEpoch>")]
    #[serde(with = "crate::lotus_json")]
    pub effective_network_time: ChainEpoch,

    #[schemars(with = "LotusJson<BigInt>")]
    #[serde(with = "crate::lotus_json")]
    pub effective_baseline_power: BigInt,

    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub this_epoch_reward: TokenAmount,

    #[schemars(with = "LotusJson<FilterEstimate>")]
    #[serde(with = "crate::lotus_json")]
    pub this_epoch_reward_smoothed: FilterEstimate,

    #[schemars(with = "LotusJson<BigInt>")]
    #[serde(with = "crate::lotus_json")]
    pub this_epoch_baseline_power: BigInt,

    #[schemars(with = "LotusJson<ChainEpoch>")]
    #[serde(with = "crate::lotus_json")]
    pub epoch: ChainEpoch,

    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub total_storage_power_reward: TokenAmount,

    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub simple_total: TokenAmount,

    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub baseline_total: TokenAmount,
}

macro_rules! impl_reward_state_lotus_json {
    ($($version:ident),*) => {
        impl HasLotusJson for State {
            type LotusJson = RewardStateLotusJson;

            #[cfg(test)]
            fn snapshots() -> Vec<(serde_json::Value, Self)> {
                vec![(
                    json!({
                        "cumsum_baseline": "1",
                        "cumsum_realized": "1",
                        "effective_network_time": 1,
                        "effective_baseline_power": "1",
                        "this_epoch_reward": "1",
                        "this_epoch_reward_smoothed": {
                            "position": "1",
                            "velocity": "1",
                        },
                        "this_epoch_baseline_power": "1",
                        "epoch": 1,
                        "total_storage_power_reward": "1",
                        "simple_total": "1",
                        "baseline_total": "1",
                    }),
                    State::V16(fil_actor_reward_state::v16::State {
                        cumsum_baseline: BigInt::from(1),
                        cumsum_realized: BigInt::from(1),
                        effective_network_time: 1,
                        effective_baseline_power: BigInt::from(1),
                        this_epoch_reward: TokenAmount::from_atto(1).into(),
                        this_epoch_reward_smoothed: FilterEstimate {
                            position: BigInt::from(1),
                            velocity: BigInt::from(1),
                        },
                        this_epoch_baseline_power: BigInt::from(1),
                        epoch: 1,
                        total_storage_power_reward: TokenAmount::from_atto(1).into(),
                        simple_total: TokenAmount::from_atto(1).into(),
                        baseline_total: TokenAmount::from_atto(1).into(),
                    }),
                )]
            }

            fn into_lotus_json(self) -> Self::LotusJson {
                match self {
                     $(
                        State::$version(state) => RewardStateLotusJson {
                            cumsum_baseline: state.cumsum_baseline.into(),
                            cumsum_realized: state.cumsum_realized.into(),
                            effective_network_time: state.effective_network_time,
                            effective_baseline_power: state.effective_baseline_power.into(),
                            this_epoch_reward: state.this_epoch_reward.into(),
                            this_epoch_reward_smoothed: FilterEstimate {
                                position: state.this_epoch_reward_smoothed.position,
                                velocity: state.this_epoch_reward_smoothed.velocity,
                            },
                            this_epoch_baseline_power: state.this_epoch_baseline_power.into(),
                            epoch: state.epoch,
                            total_storage_power_reward: state.total_storage_power_reward.into(),
                            simple_total: state.simple_total.into(),
                            baseline_total: state.baseline_total.into(),
                        },
                    )*
                }
            }

            // Default V16
            fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                State::V16(fil_actor_reward_state::v16::State {
                    cumsum_baseline: lotus_json.cumsum_baseline,
                    cumsum_realized: lotus_json.cumsum_realized,
                    effective_network_time: lotus_json.effective_network_time,
                    effective_baseline_power: lotus_json.effective_baseline_power,
                    this_epoch_reward: lotus_json.this_epoch_reward.into(),
                    this_epoch_reward_smoothed: FilterEstimate {
                        position: lotus_json.this_epoch_reward_smoothed.position,
                        velocity: lotus_json.this_epoch_reward_smoothed.velocity,
                    },
                    this_epoch_baseline_power: lotus_json.this_epoch_baseline_power,
                    epoch: lotus_json.epoch,
                    total_storage_power_reward: lotus_json.total_storage_power_reward.into(),
                    simple_total: lotus_json.simple_total.into(),
                    baseline_total: lotus_json.baseline_total.into(),
                })
            }
        }
    };
}

impl_reward_state_lotus_json!(V8, V9, V10, V11, V12, V13, V14, V15, V16);
