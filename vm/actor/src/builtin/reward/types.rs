// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::network::*;
use crate::smooth::FilterEstimate;
use address::Address;
use encoding::tuple::*;
use fil_types::StoragePower;
use num_bigint::{bigint_ser, BigInt, BigUint};
use num_traits::Pow;
use vm::TokenAmount;

/// Number of token units in an abstract "FIL" token.
/// The network works purely in the indivisible token amounts. This constant converts to a fixed decimal with more
/// human-friendly scale.
pub const TOKEN_PRECISION: u64 = 1_000_000_000_000_000_000;

lazy_static! {
    /// Target reward released to each block winner.
    pub static ref BLOCK_REWARD_TARGET: BigUint = BigUint::from(100u8) * TOKEN_PRECISION;

    pub static ref LAMBDA_NUM: BigInt = BigInt::from(EPOCH_DURATION_SECONDS) * &*LN_TWO_NUM;
    pub static ref LAMBDA_DEN: BigInt = BigInt::from(6*SECONDS_IN_YEAR) * &*LN_TWO_DEN;

    // These numbers are placeholders, but should be in units of attoFIL, 10^-18 FIL
    /// 100M for testnet, PARAM_FINISH
    pub static ref SIMPLE_TOTAL: BigInt = BigInt::from(100).pow(6u8) * BigInt::from(1).pow(18u8);
    /// 900M for testnet, PARAM_FINISH
    pub static ref BASELINE_TOTAL: BigInt = BigInt::from(900).pow(6u8) * BigInt::from(1).pow(18u8);

    // The following are the numerator and denominator of -ln(1/2)=ln(2),
    // represented as a rational with sufficient precision.
    pub static ref LN_TWO_NUM: BigInt = BigInt::from(6_931_471_805_599_453_094_172_321_215u128);
    pub static ref LN_TWO_DEN: BigInt = BigInt::from(10_000_000_000_000_000_000_000_000_000u128);
}

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
