// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{SectorStorageWeightDesc, SectorTermination};
use crate::{reward, StoragePower};
use clock::ChainEpoch;
use num_bigint::BigInt;
use num_traits::FromPrimitive;
use std::convert::TryFrom;
use vm::TokenAmount;

/// The time a miner has to respond to a surprise PoSt challenge.
pub const WINDOWED_POST_CHALLENGE_DURATION: ChainEpoch = ChainEpoch(240); // ~2 hours @ 30 second epochs. PARAM_FINISH

/// The number of consecutive failures to meet a surprise PoSt challenge before a miner is terminated.
pub const WINDOWED_POST_FAILURE_LIMIT: i64 = 3; // PARAM_FINISH

/// Minimum number of registered miners for the minimum miner size limit to effectively limit consensus power.
pub const CONSENSUS_MINER_MIN_MINERS: usize = 3;

/// Maximum age of a block header used as proof of a consensus fault to appear in the chain.
pub const CONSENSUS_FAULT_REPORTING_WINDOW: ChainEpoch = ChainEpoch(2880); // 1 day @ 30 second epochs.

lazy_static! {
    /// Multiplier on sector pledge requirement.
    pub static ref PLEDGE_FACTOR: BigInt = BigInt::from(3); // PARAM_FINISH

    /// Total expected block reward per epoch (per-winner reward * expected winners), as input to pledge requirement.
    pub static ref EPOCH_TOTAL_EXPECTED_REWARD: BigInt = reward::BLOCK_REWARD_TARGET.clone() * 5; // PARAM_FINISH

    /// Minimum power of an individual miner to meet the threshold for leader election.
    pub static ref CONSENSUS_MINER_MIN_POWER: StoragePower = StoragePower::from(2 << 30); // placeholder

}

pub(super) fn pledge_penalty_for_sector_termination(
    _pledge: &TokenAmount,
    _term_type: SectorTermination,
) -> TokenAmount {
    // PARAM_FINISH
    TokenAmount::new(0)
}

pub fn consensus_power_for_weight(weight: &SectorStorageWeightDesc) -> StoragePower {
    StoragePower::from_u64(weight.sector_size as u64).unwrap()
}

pub fn pledge_for_weight(
    weight: &SectorStorageWeightDesc,
    network_power: &StoragePower,
) -> TokenAmount {
    let numerator = (weight.sector_size as u64)
        * weight.duration.0
        * &*EPOCH_TOTAL_EXPECTED_REWARD
        * &*PLEDGE_FACTOR;
    let denominator = network_power;

    TokenAmount::try_from(numerator / denominator).expect("all values should be positive")
}
