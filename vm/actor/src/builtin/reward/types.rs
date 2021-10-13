// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::smooth::FilterEstimate;
use address::Address;
use encoding::tuple::*;
use fil_types::StoragePower;
use num_bigint::bigint_ser;
use vm::TokenAmount;

#[derive(Clone, Debug, PartialEq, Serialize_tuple, Deserialize_tuple)]
pub struct AwardBlockRewardParams {
    pub miner: Address,
    #[serde(with = "bigint_ser")]
    pub penalty: TokenAmount,
    #[serde(with = "bigint_ser")]
    pub gas_reward: TokenAmount,
    pub win_count: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize_tuple, Deserialize_tuple)]
pub struct ThisEpochRewardReturn {
    // * Removed this_epoch_reward in v2
    pub this_epoch_reward_smoothed: FilterEstimate,
    #[serde(with = "bigint_ser")]
    pub this_epoch_baseline_power: StoragePower,
}
