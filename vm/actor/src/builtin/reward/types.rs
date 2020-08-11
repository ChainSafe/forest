// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::smooth::FilterEstimate;
use address::Address;
use encoding::tuple::*;
use fil_types::StoragePower;
use num_bigint::bigint_ser;
use vm::TokenAmount;

/// Number of token units in an abstract "FIL" token.
/// The network works purely in the indivisible token amounts. This constant converts to a fixed decimal with more
/// human-friendly scale.
pub const TOKEN_PRECISION: u64 = 1_000_000_000_000_000_000;

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
    #[serde(with = "bigint_ser")]
    pub this_epoch_reward: TokenAmount,
    pub this_epoch_reward_smoothed: FilterEstimate,
    #[serde(with = "bigint_ser")]
    pub this_epoch_baseline_power: StoragePower,
}
