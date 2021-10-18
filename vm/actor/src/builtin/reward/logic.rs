// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::expneg::expneg;
use crate::math::PRECISION;
use clock::ChainEpoch;
use fil_types::{StoragePower, FILECOIN_PRECISION};
use num_bigint::{BigInt, Integer};
use std::str::FromStr;
use vm::TokenAmount;

lazy_static! {
    /// Floor(e^(ln[1 + 200%] / epochsInYear) * 2^128
    /// Q.128 formatted number such that f(epoch) = baseExponent^epoch grows 200% in one
    /// year of epochs
    /// Calculation here: https://www.wolframalpha.com/input/?i=IntegerPart%5BExp%5BLog%5B1%2B200%25%5D%2F%28%28365+days%29%2F%2830+seconds%29%29%5D*2%5E128%5D
    pub static ref BASELINE_EXPONENT: StoragePower =
        StoragePower::from_str("340282591298641078465964189926313473653").unwrap();

    // 2.5057116798121726 EiB
    pub static ref BASELINE_INITIAL_VALUE: StoragePower = StoragePower::from(2_888_888_880_000_000_000u128);

    /// 1EiB
    pub static ref INIT_BASELINE_POWER: StoragePower =
    ((BASELINE_INITIAL_VALUE.clone() << (2*PRECISION)) / &*BASELINE_EXPONENT) >> PRECISION;

    /// 330M for mainnet
    pub(super) static ref SIMPLE_TOTAL: BigInt = BigInt::from(330_000_000) * FILECOIN_PRECISION;
    /// 770M for mainnet
    pub(super) static ref BASELINE_TOTAL: BigInt = BigInt::from(770_000_000) * FILECOIN_PRECISION;
    /// expLamSubOne = e^lambda - 1
    /// for Q.128: int(expLamSubOne * 2^128)
    static ref EXP_LAM_SUB_ONE: BigInt = BigInt::from(37396273494747879394193016954629u128);
    /// lambda = ln(2) / (6 * epochsInYear)
    /// for Q.128: int(lambda * 2^128)
    static ref LAMBDA: BigInt = BigInt::from(37396271439864487274534522888786u128);
}

/// Compute BaselinePower(t) from BaselinePower(t-1) with an additional multiplication
/// of the base exponent.
pub(crate) fn baseline_power_from_prev(prev_power: &StoragePower) -> StoragePower {
    (prev_power * &*BASELINE_EXPONENT) >> PRECISION
}

/// Computes RewardTheta which is is precise fractional value of effectiveNetworkTime.
/// The effectiveNetworkTime is defined by CumsumBaselinePower(theta) == CumsumRealizedPower
/// As baseline power is defined over integers and the RewardTheta is required to be fractional,
/// we perform linear interpolation between CumsumBaseline(⌊theta⌋) and CumsumBaseline(⌈theta⌉).
/// The effectiveNetworkTime argument is ceiling of theta.
/// The result is a fractional effectiveNetworkTime (theta) in Q.128 format.
pub(crate) fn compute_r_theta(
    effective_network_time: ChainEpoch,
    baseline_power_at_effective_network_time: &BigInt,
    cumsum_realized: &BigInt,
    cumsum_baseline: &BigInt,
) -> BigInt {
    if effective_network_time != 0 {
        let reward_theta = BigInt::from(effective_network_time) << PRECISION;
        let diff = ((cumsum_baseline - cumsum_realized) << PRECISION)
            .div_floor(baseline_power_at_effective_network_time);

        reward_theta - diff
    } else {
        Default::default()
    }
}

/// Computes a reward for all expected leaders when effective network time changes
/// from prevTheta to currTheta. Inputs are in Q.128 format
pub(crate) fn compute_reward(
    epoch: ChainEpoch,
    prev_theta: BigInt,
    curr_theta: BigInt,
    simple_total: &BigInt,
    baseline_total: &BigInt,
) -> TokenAmount {
    let mut simple_reward = simple_total * &*EXP_LAM_SUB_ONE;
    let epoch_lam = &*LAMBDA * epoch;

    simple_reward *= expneg(&epoch_lam);
    simple_reward >>= PRECISION;

    let baseline_reward = compute_baseline_supply(curr_theta, baseline_total)
        - compute_baseline_supply(prev_theta, baseline_total);

    (simple_reward + baseline_reward) >> PRECISION
}

/// Computes baseline supply based on theta in Q.128 format.
/// Return is in Q.128 format
fn compute_baseline_supply(theta: BigInt, baseline_total: &BigInt) -> BigInt {
    let theta_lam = (theta * &*LAMBDA) >> PRECISION;

    let etl = expneg(&theta_lam);

    let one = BigInt::from(1) << PRECISION;
    let one_sub = one - etl;

    one_sub * baseline_total
}
