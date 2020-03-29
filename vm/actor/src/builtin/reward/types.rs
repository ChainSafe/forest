// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::VestingFunction;
use address::Address;
use clock::ChainEpoch;
use num_bigint::biguint_ser::{BigUintDe, BigUintSer};
use num_bigint::BigUint;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
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

#[derive(Clone, Debug, PartialEq)]
pub struct AwardBlockRewardParams {
    pub miner: Address,
    pub penalty: TokenAmount,
    pub gas_reward: TokenAmount,
    pub ticket_count: u64,
}

impl Serialize for AwardBlockRewardParams {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (
            &self.miner,
            BigUintSer(&self.penalty),
            BigUintSer(&self.gas_reward),
            &self.ticket_count,
        )
            .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for AwardBlockRewardParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (miner, BigUintDe(penalty), BigUintDe(gas_reward), ticket_count) =
            Deserialize::deserialize(deserializer)?;
        Ok(Self {
            miner,
            penalty,
            gas_reward,
            ticket_count,
        })
    }
}
