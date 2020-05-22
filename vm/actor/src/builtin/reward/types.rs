// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::VestingFunction;
use address::Address;
use clock::ChainEpoch;
use encoding::tuple::*;
use num_bigint::{biguint_ser, BigUint};
use vm::TokenAmount;

/// Number of token units in an abstract "FIL" token.
/// The network works purely in the indivisible token amounts. This constant converts to a fixed decimal with more
/// human-friendly scale.
pub const TOKEN_PRECISION: u64 = 1_000_000_000_000_000_000;

lazy_static! {
    /// Target reward released to each block winner.
    pub static ref BLOCK_REWARD_TARGET: BigUint = BigUint::from(100u8) * TOKEN_PRECISION;
}

pub(super) const REWARD_VESTING_FUNCTION: VestingFunction = VestingFunction::None;
pub(super) const REWARD_VESTING_PERIOD: ChainEpoch = 0;

#[derive(Clone, Debug, PartialEq, Serialize_tuple, Deserialize_tuple)]
pub struct AwardBlockRewardParams {
    pub miner: Address,
    #[serde(with = "biguint_ser")]
    pub penalty: TokenAmount,
    #[serde(with = "biguint_ser")]
    pub gas_reward: TokenAmount,
    pub ticket_count: u64,
}
