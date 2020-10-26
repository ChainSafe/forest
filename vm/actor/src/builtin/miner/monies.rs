// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{
    math::PRECISION,
    network::EPOCHS_IN_DAY,
    smooth::{self, FilterEstimate},
    TokenAmount,
};
use clock::ChainEpoch;
use fil_types::{NetworkVersion, StoragePower};
use num_bigint::{BigInt, Integer};
use num_traits::Zero;
use std::cmp;

// IP = IPBase(precommit time) + AdditionalIP(precommit time)
// IPBase(t) = BR(t, InitialPledgeProjectionPeriod)
// AdditionalIP(t) = LockTarget(t)*PledgeShare(t)
// LockTarget = (LockTargetFactorNum / LockTargetFactorDenom) * FILCirculatingSupply(t)
// PledgeShare(t) = sectorQAPower / max(BaselinePower(t), NetworkQAPower(t))
// PARAM_FINISH
const PRE_COMMIT_DEPOSIT_FACTOR: u64 = 20;
const INITIAL_PLEDGE_FACTOR: u64 = 20;
pub const PRE_COMMIT_DEPOSIT_PROJECTION_PERIOD: i64 =
    (PRE_COMMIT_DEPOSIT_FACTOR as ChainEpoch) * EPOCHS_IN_DAY;
pub const INITIAL_PLEDGE_PROJECTION_PERIOD: i64 =
    (INITIAL_PLEDGE_FACTOR as ChainEpoch) * EPOCHS_IN_DAY;

lazy_static! {
    static ref LOCK_TARGET_FACTOR_NUM: BigInt = BigInt::from(3);
    static ref LOCK_TARGET_FACTOR_DENOM: BigInt = BigInt::from(10);

    /// Cap on initial pledge requirement for sectors during the Space Race network.
    /// The target is 1 FIL (10**18 attoFIL) per 32GiB.
    /// This does not divide evenly, so the result is fractionally smaller.
    static ref SPACE_RACE_INITIAL_PLEDGE_MAX_PER_BYTE: BigInt =
        BigInt::from(10_u64.pow(18) / (32 << 30));
}

// FF = BR(t, DeclaredFaultProjectionPeriod)
// projection period of 2.14 days:  2880 * 2.14 = 6163.2.  Rounded to nearest epoch 6163
const DECLARED_FAULT_FACTOR_NUM_V0: i64 = 214;
const DECLARED_FAULT_FACTOR_NUM_V3: i64 = 351;
const DECLARED_FAULT_FACTOR_DENOM: i64 = 100;
pub const DECLARED_FAULT_PROJECTION_PERIOD_V0: ChainEpoch =
    (EPOCHS_IN_DAY * DECLARED_FAULT_FACTOR_NUM_V0) / DECLARED_FAULT_FACTOR_DENOM;
pub const DECLARED_FAULT_PROJECTION_PERIOD_V3: ChainEpoch =
    (EPOCHS_IN_DAY * DECLARED_FAULT_FACTOR_NUM_V3) / DECLARED_FAULT_FACTOR_DENOM;

// SP = BR(t, UndeclaredFaultProjectionPeriod)
const UNDECLARED_FAULT_FACTOR_NUM_V0: i64 = 50;
const UNDECLARED_FAULT_FACTOR_NUM_V1: i64 = 35;
const UNDECLARED_FAULT_FACTOR_DENOM: i64 = 10;
pub const UNDECLARED_FAULT_PROJECTION_PERIOD_V0: i64 =
    (EPOCHS_IN_DAY * UNDECLARED_FAULT_FACTOR_NUM_V0) / UNDECLARED_FAULT_FACTOR_DENOM;
pub const UNDECLARED_FAULT_PROJECTION_PERIOD_V1: i64 =
    (EPOCHS_IN_DAY * UNDECLARED_FAULT_FACTOR_NUM_V1) / UNDECLARED_FAULT_FACTOR_DENOM;

// Maximum number of days of BR a terminated sector can be penalized
pub const TERMINATION_LIFETIME_CAP: ChainEpoch = 70;

/// This is the BR(t) value of the given sector for the current epoch.
/// It is the expected reward this sector would pay out over a one day period.
/// BR(t) = CurrEpochReward(t) * SectorQualityAdjustedPower * EpochsInDay / TotalNetworkQualityAdjustedPower(t)
pub fn expected_reward_for_power(
    reward_estimate: &FilterEstimate,
    network_qa_power_estimate: &FilterEstimate,
    qa_sector_power: &StoragePower,
    projection_duration: ChainEpoch,
) -> TokenAmount {
    let network_qa_power_smoothed = network_qa_power_estimate.estimate();

    if network_qa_power_smoothed.is_zero() {
        return reward_estimate.estimate();
    }

    let expected_reward_for_proving_period = smooth::extrapolated_cum_sum_of_ratio(
        projection_duration,
        0,
        reward_estimate,
        network_qa_power_estimate,
    );
    let br = qa_sector_power * expected_reward_for_proving_period; // Q.0 * Q.128 => Q.128
    br >> PRECISION
}

// This is the FF(t) penalty for a sector expected to be in the fault state either because the fault was declared or because
// it has been previously detected by the network.
// FF(t) = DeclaredFaultFactor * BR(t)
pub fn pledge_penalty_for_declared_fault(
    reward_estimate: &FilterEstimate,
    network_qa_power_estimate: &FilterEstimate,
    qa_sector_power: &StoragePower,
    network_version: NetworkVersion,
) -> TokenAmount {
    let projection_period = if network_version < NetworkVersion::V3 {
        DECLARED_FAULT_PROJECTION_PERIOD_V0
    } else {
        DECLARED_FAULT_PROJECTION_PERIOD_V3
    };
    expected_reward_for_power(
        reward_estimate,
        network_qa_power_estimate,
        qa_sector_power,
        projection_period,
    )
}

/// This is the SP(t) penalty for a newly faulty sector that has not been declared.
/// SP(t) = UndeclaredFaultFactor * BR(t)
pub fn pledge_penalty_for_undeclared_fault(
    reward_estimate: &FilterEstimate,
    network_qa_power_estimate: &FilterEstimate,
    qa_sector_power: &StoragePower,
    network_version: NetworkVersion,
) -> TokenAmount {
    let projection_period = if network_version < NetworkVersion::V3 {
        UNDECLARED_FAULT_PROJECTION_PERIOD_V0
    } else {
        UNDECLARED_FAULT_PROJECTION_PERIOD_V1
    };
    expected_reward_for_power(
        reward_estimate,
        network_qa_power_estimate,
        qa_sector_power,
        projection_period,
    )
}

/// Penalty to locked pledge collateral for the termination of a sector before scheduled expiry.
/// SectorAge is the time between the sector's activation and termination.
pub fn pledge_penalty_for_termination(
    day_reward_at_activation: &TokenAmount,
    twenty_day_reward_at_activation: &TokenAmount,
    mut sector_age: ChainEpoch,
    reward_estimate: &FilterEstimate,
    network_qa_power_estimate: &FilterEstimate,
    qa_sector_power: &StoragePower,
    network_version: NetworkVersion,
) -> TokenAmount {
    // max(SP(t), BR(StartEpoch, 20d) + BR(StartEpoch, 1d)*min(SectorAgeInDays, 70))
    // and sectorAgeInDays = sectorAge / EpochsInDay
    if network_version >= NetworkVersion::V1 {
        sector_age /= 2;
    }
    let capped_sector_age = BigInt::from(cmp::min(
        sector_age,
        TERMINATION_LIFETIME_CAP * EPOCHS_IN_DAY,
    ));

    cmp::max(
        pledge_penalty_for_undeclared_fault(
            reward_estimate,
            network_qa_power_estimate,
            qa_sector_power,
            network_version,
        ),
        twenty_day_reward_at_activation
            + (day_reward_at_activation * capped_sector_age) / EPOCHS_IN_DAY,
    )
}

/// Computes the PreCommit Deposit given sector qa weight and current network conditions.
/// PreCommit Deposit = 20 * BR(t)
pub fn pre_commit_deposit_for_power(
    reward_estimate: &FilterEstimate,
    network_qa_power_estimate: &FilterEstimate,
    qa_sector_power: &StoragePower,
) -> TokenAmount {
    expected_reward_for_power(
        reward_estimate,
        network_qa_power_estimate,
        qa_sector_power,
        PRE_COMMIT_DEPOSIT_PROJECTION_PERIOD,
    )
}

// Computes the pledge requirement for committing new quality-adjusted power to the network, given the current
// total power, total pledge commitment, epoch block reward, and circulating token supply.
// In plain language, the pledge requirement is a multiple of the block reward expected to be earned by the
// newly-committed power, holding the per-epoch block reward constant (though in reality it will change over time).
pub fn initial_pledge_for_power(
    qa_power: &StoragePower,
    baseline_power: &StoragePower,
    reward_estimate: &FilterEstimate,
    network_qa_power_estimate: &FilterEstimate,
    network_circulating_supply_smoothed: &TokenAmount,
) -> TokenAmount {
    let network_qa_power = network_qa_power_estimate.estimate();
    let ip_base = expected_reward_for_power(
        reward_estimate,
        network_qa_power_estimate,
        qa_power,
        INITIAL_PLEDGE_PROJECTION_PERIOD,
    );

    let lock_target_num = &*LOCK_TARGET_FACTOR_NUM * network_circulating_supply_smoothed;
    let lock_target_denom = &*LOCK_TARGET_FACTOR_DENOM;
    let pledge_share_num = qa_power;
    let pledge_share_denom = cmp::max(cmp::max(&network_qa_power, baseline_power), qa_power); // use qaPower in case others are 0
    let additional_ip_num: TokenAmount = &lock_target_num * pledge_share_num;
    let additional_ip_denom = lock_target_denom * pledge_share_denom;
    let additional_ip = additional_ip_num.div_floor(&additional_ip_denom);

    let nominal_pledge = ip_base + additional_ip;
    let space_race_pledge_cap = &*SPACE_RACE_INITIAL_PLEDGE_MAX_PER_BYTE * qa_power;

    cmp::min(nominal_pledge, space_race_pledge_cap)
}
