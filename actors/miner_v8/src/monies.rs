// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::cmp::{self, max};

use fil_actors_runtime_v8::network::EPOCHS_IN_DAY;
use fil_actors_runtime_v8::EXPECTED_LEADERS_PER_EPOCH;
use fvm_shared::bigint::{BigInt, Integer};
use fvm_shared::clock::ChainEpoch;
use fvm_shared::econ::TokenAmount;
use fvm_shared::math::PRECISION;
use fvm_shared::sector::StoragePower;
use fvm_shared::smooth::{self, FilterEstimate};
use lazy_static::lazy_static;
use num_traits::Zero;

use super::{VestSpec, REWARD_VESTING_SPEC};
use crate::detail::*;

/// Projection period of expected sector block reward for deposit required to pre-commit a sector.
/// This deposit is lost if the pre-commitment is not timely followed up by a commitment proof.
const PRE_COMMIT_DEPOSIT_FACTOR: u64 = 20;

/// Projection period of expected sector block rewards for storage pledge required to commit a sector.
/// This pledge is lost if a sector is terminated before its full committed lifetime.
pub const INITIAL_PLEDGE_FACTOR: u64 = 20;

pub const PRE_COMMIT_DEPOSIT_PROJECTION_PERIOD: i64 =
    (PRE_COMMIT_DEPOSIT_FACTOR as ChainEpoch) * EPOCHS_IN_DAY;
pub const INITIAL_PLEDGE_PROJECTION_PERIOD: i64 =
    (INITIAL_PLEDGE_FACTOR as ChainEpoch) * EPOCHS_IN_DAY;

const LOCK_TARGET_FACTOR_NUM: u32 = 3;
const LOCK_TARGET_FACTOR_DENOM: u32 = 10;

pub const TERMINATION_REWARD_FACTOR_NUM: u32 = 1;
pub const TERMINATION_REWARD_FACTOR_DENOM: u32 = 2;

// * go impl has 75/100 but this is just simplified
const LOCKED_REWARD_FACTOR_NUM: u32 = 3;
const LOCKED_REWARD_FACTOR_DENOM: u32 = 4;

lazy_static! {
    /// Cap on initial pledge requirement for sectors during the Space Race network.
    /// The target is 1 FIL (10**18 attoFIL) per 32GiB.
    /// This does not divide evenly, so the result is fractionally smaller.
    static ref INITIAL_PLEDGE_MAX_PER_BYTE: TokenAmount =
        TokenAmount::from_whole(1).div_floor(32i64 << 30);

    /// Base reward for successfully disputing a window posts proofs.
    pub static ref BASE_REWARD_FOR_DISPUTED_WINDOW_POST: TokenAmount =
        TokenAmount::from_whole(4);

    /// Base penalty for a successful disputed window post proof.
    pub static ref BASE_PENALTY_FOR_DISPUTED_WINDOW_POST: TokenAmount =
        TokenAmount::from_whole(20);
}
// FF + 2BR
const INVALID_WINDOW_POST_PROJECTION_PERIOD: ChainEpoch =
    CONTINUED_FAULT_PROJECTION_PERIOD + 2 * EPOCHS_IN_DAY;

// Projection period of expected daily sector block reward penalised when a fault is continued after initial detection.
// This guarantees that a miner pays back at least the expected block reward earned since the last successful PoSt.
// The network conservatively assumes the sector was faulty since the last time it was proven.
// This penalty is currently overly punitive for continued faults.
// FF = BR(t, ContinuedFaultProjectionPeriod)
const CONTINUED_FAULT_FACTOR_NUM: i64 = 351;
const CONTINUED_FAULT_FACTOR_DENOM: i64 = 100;
pub const CONTINUED_FAULT_PROJECTION_PERIOD: ChainEpoch =
    (EPOCHS_IN_DAY * CONTINUED_FAULT_FACTOR_NUM) / CONTINUED_FAULT_FACTOR_DENOM;

const TERMINATION_PENALTY_LOWER_BOUND_PROJECTIONS_PERIOD: ChainEpoch = (EPOCHS_IN_DAY * 35) / 10;

// Maximum number of lifetime days penalized when a sector is terminated.
pub const TERMINATION_LIFETIME_CAP: ChainEpoch = 140;

// Multiplier of whole per-winner rewards for a consensus fault penalty.
const CONSENSUS_FAULT_FACTOR: u64 = 5;

/// The projected block reward a sector would earn over some period.
/// Also known as "BR(t)".
/// BR(t) = ProjectedRewardFraction(t) * SectorQualityAdjustedPower
/// ProjectedRewardFraction(t) is the sum of estimated reward over estimated total power
/// over all epochs in the projection period [t t+projectionDuration]
pub fn expected_reward_for_power(
    reward_estimate: &FilterEstimate,
    network_qa_power_estimate: &FilterEstimate,
    qa_sector_power: &StoragePower,
    projection_duration: ChainEpoch,
) -> TokenAmount {
    let network_qa_power_smoothed = network_qa_power_estimate.estimate();

    if network_qa_power_smoothed.is_zero() {
        return TokenAmount::from_atto(reward_estimate.estimate());
    }

    let expected_reward_for_proving_period = smooth::extrapolated_cum_sum_of_ratio(
        projection_duration,
        0,
        reward_estimate,
        network_qa_power_estimate,
    );
    let br128 = qa_sector_power * expected_reward_for_proving_period; // Q.0 * Q.128 => Q.128
    TokenAmount::from_atto(std::cmp::max(br128 >> PRECISION, Default::default()))
}

pub mod detail {
    use super::*;

    lazy_static! {
        pub static ref BATCH_BALANCER: TokenAmount = TokenAmount::from_nano(5);
    }

    // BR but zero values are clamped at 1 attofil
    // Some uses of BR (PCD, IP) require a strictly positive value for BR derived values so
    // accounting variables can be used as succinct indicators of miner activity.
    pub fn expected_reward_for_power_clamped_at_atto_fil(
        reward_estimate: &FilterEstimate,
        network_qa_power_estimate: &FilterEstimate,
        qa_sector_power: &StoragePower,
        projection_duration: ChainEpoch,
    ) -> TokenAmount {
        let br = expected_reward_for_power(
            reward_estimate,
            network_qa_power_estimate,
            qa_sector_power,
            projection_duration,
        );
        if br.le(&TokenAmount::zero()) {
            TokenAmount::from_atto(1)
        } else {
            br
        }
    }
}

// func ExpectedRewardForPowerClampedAtAttoFIL(rewardEstimate, networkQAPowerEstimate smoothing.FilterEstimate, qaSectorPower abi.StoragePower, projectionDuration abi.ChainEpoch) abi.TokenAmount {
// 	br := ExpectedRewardForPower(rewardEstimate, networkQAPowerEstimate, qaSectorPower, projectionDuration)
// 	if br.LessThanEqual(big.Zero()) {
// 		br = abi.NewTokenAmount(1)
// 	}
// 	return br
// }

/// The penalty for a sector continuing faulty for another proving period.
/// It is a projection of the expected reward earned by the sector.
/// Also known as "FF(t)"
pub fn pledge_penalty_for_continued_fault(
    reward_estimate: &FilterEstimate,
    network_qa_power_estimate: &FilterEstimate,
    qa_sector_power: &StoragePower,
) -> TokenAmount {
    expected_reward_for_power(
        reward_estimate,
        network_qa_power_estimate,
        qa_sector_power,
        CONTINUED_FAULT_PROJECTION_PERIOD,
    )
}

/// This is the SP(t) penalty for a newly faulty sector that has not been declared.
/// SP(t) = UndeclaredFaultFactor * BR(t)
pub fn pledge_penalty_for_termination_lower_bound(
    reward_estimate: &FilterEstimate,
    network_qa_power_estimate: &FilterEstimate,
    qa_sector_power: &StoragePower,
) -> TokenAmount {
    expected_reward_for_power(
        reward_estimate,
        network_qa_power_estimate,
        qa_sector_power,
        TERMINATION_PENALTY_LOWER_BOUND_PROJECTIONS_PERIOD,
    )
}

/// Penalty to locked pledge collateral for the termination of a sector before scheduled expiry.
/// SectorAge is the time between the sector's activation and termination.
#[allow(clippy::too_many_arguments)]
pub fn pledge_penalty_for_termination(
    day_reward: &TokenAmount,
    sector_age: ChainEpoch,
    twenty_day_reward_at_activation: &TokenAmount,
    network_qa_power_estimate: &FilterEstimate,
    qa_sector_power: &StoragePower,
    reward_estimate: &FilterEstimate,
    replaced_day_reward: &TokenAmount,
    replaced_sector_age: ChainEpoch,
) -> TokenAmount {
    // max(SP(t), BR(StartEpoch, 20d) + BR(StartEpoch, 1d) * terminationRewardFactor * min(SectorAgeInDays, 140))
    // and sectorAgeInDays = sectorAge / EpochsInDay
    let lifetime_cap = TERMINATION_LIFETIME_CAP * EPOCHS_IN_DAY;
    let capped_sector_age = std::cmp::min(sector_age, lifetime_cap);

    let mut expected_reward: TokenAmount = day_reward * capped_sector_age;

    let relevant_replaced_age =
        std::cmp::min(replaced_sector_age, lifetime_cap - capped_sector_age);

    expected_reward += replaced_day_reward * relevant_replaced_age;

    let penalized_reward = expected_reward * TERMINATION_REWARD_FACTOR_NUM;
    let penalized_reward = penalized_reward.div_floor(TERMINATION_REWARD_FACTOR_DENOM);

    cmp::max(
        pledge_penalty_for_termination_lower_bound(
            reward_estimate,
            network_qa_power_estimate,
            qa_sector_power,
        ),
        twenty_day_reward_at_activation + (penalized_reward.div_floor(EPOCHS_IN_DAY)),
    )
}

// The penalty for optimistically proving a sector with an invalid window PoSt.
pub fn pledge_penalty_for_invalid_windowpost(
    reward_estimate: &FilterEstimate,
    network_qa_power_estimate: &FilterEstimate,
    qa_sector_power: &StoragePower,
) -> TokenAmount {
    expected_reward_for_power(
        reward_estimate,
        network_qa_power_estimate,
        qa_sector_power,
        INVALID_WINDOW_POST_PROJECTION_PERIOD,
    ) + &*BASE_PENALTY_FOR_DISPUTED_WINDOW_POST
}

/// Computes the PreCommit deposit given sector qa weight and current network conditions.
/// PreCommit Deposit = BR(PreCommitDepositProjectionPeriod)
pub fn pre_commit_deposit_for_power(
    reward_estimate: &FilterEstimate,
    network_qa_power_estimate: &FilterEstimate,
    qa_sector_power: &StoragePower,
) -> TokenAmount {
    expected_reward_for_power_clamped_at_atto_fil(
        reward_estimate,
        network_qa_power_estimate,
        qa_sector_power,
        PRE_COMMIT_DEPOSIT_PROJECTION_PERIOD,
    )
}

/// Computes the pledge requirement for committing new quality-adjusted power to the network, given
/// the current network total and baseline power, per-epoch  reward, and circulating token supply.
/// The pledge comprises two parts:
/// - storage pledge, aka IP base: a multiple of the reward expected to be earned by newly-committed power
/// - consensus pledge, aka additional IP: a pro-rata fraction of the circulating money supply
///
/// IP = IPBase(t) + AdditionalIP(t)
/// IPBase(t) = BR(t, InitialPledgeProjectionPeriod)
/// AdditionalIP(t) = LockTarget(t)*PledgeShare(t)
/// LockTarget = (LockTargetFactorNum / LockTargetFactorDenom) * FILCirculatingSupply(t)
/// PledgeShare(t) = sectorQAPower / max(BaselinePower(t), NetworkQAPower(t))
pub fn initial_pledge_for_power(
    qa_power: &StoragePower,
    baseline_power: &StoragePower,
    reward_estimate: &FilterEstimate,
    network_qa_power_estimate: &FilterEstimate,
    circulating_supply: &TokenAmount,
) -> TokenAmount {
    let ip_base = expected_reward_for_power_clamped_at_atto_fil(
        reward_estimate,
        network_qa_power_estimate,
        qa_power,
        INITIAL_PLEDGE_PROJECTION_PERIOD,
    );

    let lock_target_num = LOCK_TARGET_FACTOR_NUM * circulating_supply.atto();
    let lock_target_denom = LOCK_TARGET_FACTOR_DENOM;
    let pledge_share_num = qa_power;
    let network_qa_power = network_qa_power_estimate.estimate();
    let pledge_share_denom = cmp::max(cmp::max(&network_qa_power, baseline_power), qa_power);
    let additional_ip_num = lock_target_num * pledge_share_num;
    let additional_ip_denom = lock_target_denom * pledge_share_denom;
    let additional_ip = additional_ip_num.div_floor(&additional_ip_denom);

    let nominal_pledge = ip_base + TokenAmount::from_atto(additional_ip);
    let pledge_cap = TokenAmount::from_atto(INITIAL_PLEDGE_MAX_PER_BYTE.atto() * qa_power);

    cmp::min(nominal_pledge, pledge_cap)
}

pub fn consensus_fault_penalty(this_epoch_reward: TokenAmount) -> TokenAmount {
    (this_epoch_reward * CONSENSUS_FAULT_FACTOR).div_floor(EXPECTED_LEADERS_PER_EPOCH)
}

/// Returns the amount of a reward to vest, and the vesting schedule, for a reward amount.
pub fn locked_reward_from_reward(reward: TokenAmount) -> (TokenAmount, &'static VestSpec) {
    let lock_amount = (reward * LOCKED_REWARD_FACTOR_NUM).div_floor(LOCKED_REWARD_FACTOR_DENOM);
    (lock_amount, &REWARD_VESTING_SPEC)
}

const BATCH_DISCOUNT_NUM: u32 = 1;
const BATCH_DISCOUNT_DENOM: u32 = 20;

lazy_static! {
    static ref ESTIMATED_SINGLE_PROVE_COMMIT_GAS_USAGE: BigInt = BigInt::from(49299973);
    static ref ESTIMATED_SINGLE_PRE_COMMIT_GAS_USAGE: BigInt = BigInt::from(16433324);
}

pub fn aggregate_prove_commit_network_fee(
    aggregate_size: i64,
    base_fee: &TokenAmount,
) -> TokenAmount {
    aggregate_network_fee(
        aggregate_size,
        &ESTIMATED_SINGLE_PROVE_COMMIT_GAS_USAGE,
        base_fee,
    )
}

pub fn aggregate_pre_commit_network_fee(
    aggregate_size: i64,
    base_fee: &TokenAmount,
) -> TokenAmount {
    aggregate_network_fee(
        aggregate_size,
        &ESTIMATED_SINGLE_PRE_COMMIT_GAS_USAGE,
        base_fee,
    )
}

pub fn aggregate_network_fee(
    aggregate_size: i64,
    gas_usage: &BigInt,
    base_fee: &TokenAmount,
) -> TokenAmount {
    let effective_gas_fee = max(base_fee, &*BATCH_BALANCER);
    let network_fee_num = effective_gas_fee * gas_usage * aggregate_size * BATCH_DISCOUNT_NUM;
    network_fee_num.div_floor(BATCH_DISCOUNT_DENOM)
}
