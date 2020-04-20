// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{SectorStorageWeightDesc, SectorTermination};
use crate::{reward, StoragePower};
use clock::ChainEpoch;
use num_bigint::{BigInt, BigUint};
use num_traits::{FromPrimitive, Pow};
use runtime::ConsensusFaultType;
use vm::TokenAmount;

/// The time a miner has to respond to a surprise PoSt challenge.
pub const WINDOWED_POST_CHALLENGE_DURATION: ChainEpoch = 240; // ~2 hours @ 30 second epochs. PARAM_FINISH

/// The number of consecutive failures to meet a surprise PoSt challenge before a miner is terminated.
pub const WINDOWED_POST_FAILURE_LIMIT: i64 = 3; // PARAM_FINISH

/// Minimum number of registered miners for the minimum miner size limit to effectively limit consensus power.
pub const CONSENSUS_MINER_MIN_MINERS: usize = 3;

/// Maximum age of a block header used as proof of a consensus fault to appear in the chain.
pub const CONSENSUS_FAULT_REPORTING_WINDOW: ChainEpoch = 2880; // 1 day @ 30 second epochs.

lazy_static! {
    /// Multiplier on sector pledge requirement.
    pub static ref PLEDGE_FACTOR: BigUint = BigUint::from(3u8); // PARAM_FINISH

    /// Total expected block reward per epoch (per-winner reward * expected winners), as input to pledge requirement.
    pub static ref EPOCH_TOTAL_EXPECTED_REWARD: BigUint = reward::BLOCK_REWARD_TARGET.clone() * 5u8; // PARAM_FINISH

    /// Minimum power of an individual miner to meet the threshold for leader election.
    pub static ref CONSENSUS_MINER_MIN_POWER: StoragePower = StoragePower::from_i32(2 << 30).unwrap(); // placeholder

}

/// Penalty to pledge collateral for the termination of an individual sector.
pub(super) fn pledge_penalty_for_sector_termination(
    _pledge: &TokenAmount,
    _term_type: SectorTermination,
) -> TokenAmount {
    // PARAM_FINISH
    TokenAmount::from(0u8)
}

// Penalty to pledge collateral for repeated failure to prove storage.
pub(super) fn pledge_penalty_for_windowed_post_failure(
    _pledge: &TokenAmount,
    _term_type: SectorTermination,
) -> TokenAmount {
    // PARAM_FINISH
    TokenAmount::from(0u8)
}

/// Penalty to pledge collateral for a consensus fault.
pub(super) fn pledge_penalty_for_consensus_fault(
    pledge: TokenAmount,
    fault_type: ConsensusFaultType,
) -> TokenAmount {
    // PARAM_FINISH: always penalise the entire pledge.
    match fault_type {
        ConsensusFaultType::DoubleForkMining => pledge,
        ConsensusFaultType::ParentGrinding => pledge,
        ConsensusFaultType::TimeOffsetMining => pledge,
    }
}

lazy_static! {
    static ref INITIAL_SLASHER_SHARE_NUM: BigInt = BigInt::from(1);
    static ref INITIAL_SLASHER_SHARE_DENOM: BigInt = BigInt::from(1000);
    static ref SLASHER_SHARE_GROWTH_RATE_NUM: BigInt = BigInt::from(102_813);
    static ref SLASHER_SHARE_GROWTH_RATE_DENOM: BigInt = BigInt::from(100_000);
}

pub(super) fn reward_for_consensus_slash_report(
    elapsed_epoch: ChainEpoch,
    collateral: TokenAmount,
) -> TokenAmount {
    // PARAM_FINISH
    // BigInt Operation
    // NUM = SLASHER_SHARE_GROWTH_RATE_NUM^elapsed_epoch * INITIAL_SLASHER_SHARE_NUM * collateral
    // DENOM = SLASHER_SHARE_GROWTH_RATE_DENOM^elapsed_epoch * INITIAL_SLASHER_SHARE_DENOM
    // slasher_amount = min(NUM/DENOM, collateral)
    let slasher_share_numerator: BigInt = SLASHER_SHARE_GROWTH_RATE_NUM.pow(elapsed_epoch);
    let slasher_share_denom: BigInt = SLASHER_SHARE_GROWTH_RATE_DENOM.pow(elapsed_epoch);

    let num: BigInt =
        slasher_share_numerator * &*INITIAL_SLASHER_SHARE_NUM * BigInt::from(collateral.clone());
    let denom = slasher_share_denom * &*INITIAL_SLASHER_SHARE_DENOM;
    std::cmp::min((num / denom).to_biguint().unwrap(), collateral)
}

pub fn consensus_power_for_weight(weight: &SectorStorageWeightDesc) -> StoragePower {
    StoragePower::from_u64(weight.sector_size as u64).unwrap()
}

pub fn pledge_for_weight(
    weight: &SectorStorageWeightDesc,
    network_power: &StoragePower,
) -> TokenAmount {
    let numerator = (weight.sector_size as u64)
        * weight.duration
        * &*EPOCH_TOTAL_EXPECTED_REWARD
        * &*PLEDGE_FACTOR;
    let denominator = network_power;

    numerator / denominator
}
