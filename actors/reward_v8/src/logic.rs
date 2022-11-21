// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::str::FromStr;

use fvm_shared::bigint::{BigInt, Integer};
use fvm_shared::clock::ChainEpoch;
use fvm_shared::econ::TokenAmount;
use fvm_shared::math::PRECISION;
use fvm_shared::sector::StoragePower;
use lazy_static::lazy_static;

use super::expneg::expneg;

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
    pub(super) static ref SIMPLE_TOTAL: TokenAmount = TokenAmount::from_whole(330_000_000);
    /// 770M for mainnet
    pub(super) static ref BASELINE_TOTAL: TokenAmount = TokenAmount::from_whole(770_000_000);
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
    simple_total: &TokenAmount,
    baseline_total: &TokenAmount,
) -> TokenAmount {
    let mut simple_reward = simple_total.atto() * &*EXP_LAM_SUB_ONE;
    let epoch_lam = &*LAMBDA * epoch;

    simple_reward *= expneg(&epoch_lam);
    simple_reward >>= PRECISION;

    let baseline_reward = compute_baseline_supply(curr_theta, baseline_total.atto())
        - compute_baseline_supply(prev_theta, baseline_total.atto());

    TokenAmount::from_atto((simple_reward + baseline_reward) >> PRECISION)
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

#[cfg(test)]
mod tests {
    const SECONDS_IN_HOUR: i64 = 60 * 60;
    const EPOCH_DURATION_IN_SECONDS: i64 = 30;
    const EPOCHS_IN_HOUR: i64 = SECONDS_IN_HOUR / EPOCH_DURATION_IN_SECONDS;
    const EPOCHS_IN_DAY: i64 = 24 * EPOCHS_IN_HOUR;
    const EPOCHS_IN_YEAR: i64 = 365 * EPOCHS_IN_DAY;

    use super::*;
    use num::BigRational;
    use num::ToPrimitive;
    use std::fs;
    use std::ops::Shl;

    // Converted from: https://github.com/filecoin-project/specs-actors/blob/d56b240af24517443ce1f8abfbdab7cb22d331f1/actors/builtin/reward/reward_logic_test.go#L18
    // x => x/(2^128)
    fn q128_to_f64(x: BigInt) -> f64 {
        let denom = BigInt::from(1u64).shl(u128::BITS);
        BigRational::new(x, denom)
            .to_f64()
            .expect("BigInt cannot be expressed as a 64bit float")
    }

    // Converted from: https://github.com/filecoin-project/specs-actors/blob/d56b240af24517443ce1f8abfbdab7cb22d331f1/actors/builtin/reward/reward_logic_test.go#L25
    #[test]
    fn test_compute_r_theta() {
        fn baseline_power_at(epoch: ChainEpoch) -> BigInt {
            (BigInt::from(epoch) + BigInt::from(1i64)) * BigInt::from(2048)
        }

        assert_eq!(
            q128_to_f64(compute_r_theta(
                1,
                &baseline_power_at(1),
                &BigInt::from(2048 + 2 * 2048 / 2),
                &BigInt::from(2048 + 2 * 2048),
            )),
            0.5
        );

        assert_eq!(
            q128_to_f64(compute_r_theta(
                1,
                &baseline_power_at(1),
                &BigInt::from(2048 + 2 * 2048 / 4),
                &BigInt::from(2048 + 2 * 2048),
            )),
            0.25
        );

        let cumsum15 = (0..16).map(baseline_power_at).sum::<BigInt>();
        assert_eq!(
            q128_to_f64(compute_r_theta(
                16,
                &baseline_power_at(16),
                &(&cumsum15 + baseline_power_at(16) / BigInt::from(4)),
                &(&cumsum15 + baseline_power_at(16)),
            )),
            15.25
        );
    }

    // Converted from: https://github.com/filecoin-project/specs-actors/blob/d56b240af24517443ce1f8abfbdab7cb22d331f1/actors/builtin/reward/reward_logic_test.go#L43
    #[test]
    fn test_baseline_reward() {
        let step = BigInt::from(5000_i64).shl(u128::BITS) - BigInt::from(77_777_777_777_i64); // offset from full integers
        let delta = BigInt::from(1_i64).shl(u128::BITS) - BigInt::from(33_333_333_333_i64); // offset from full integers

        let mut prev_theta = BigInt::from(0i64);
        let mut theta = delta;

        let mut b = String::from("t0, t1, y\n");
        let simple = compute_reward(
            0,
            BigInt::from(0i64),
            BigInt::from(0i64),
            &SIMPLE_TOTAL,
            &BASELINE_TOTAL,
        );

        for _ in 0..512 {
            let mut reward = compute_reward(
                0,
                prev_theta.clone(),
                theta.clone(),
                &SIMPLE_TOTAL,
                &BASELINE_TOTAL,
            );
            reward -= &simple;

            let prev_theta_str = &prev_theta.to_string();
            let theta_str = &theta.to_string();
            let reward_str = &reward.atto().to_string();
            b.push_str(prev_theta_str);
            b.push(',');
            b.push_str(theta_str);
            b.push(',');
            b.push_str(reward_str);
            b.push('\n');

            prev_theta += &step;
            theta += &step;
        }

        // compare test output to golden file used for golang tests; file originally located at filecoin-project/specs-actors/actors/builtin/reward/testdata/TestBaselineReward.golden (current link: https://github.com/filecoin-project/specs-actors/blob/d56b240af24517443ce1f8abfbdab7cb22d331f1/actors/builtin/reward/testdata/TestBaselineReward.golden)
        let filename = "testdata/TestBaselineReward.golden";
        let golden_contents =
            fs::read_to_string(filename).expect("Something went wrong reading the file");

        assert_eq!(golden_contents, b);
    }

    // Converted from: https://github.com/filecoin-project/specs-actors/blob/d56b240af24517443ce1f8abfbdab7cb22d331f1/actors/builtin/reward/reward_logic_test.go#L70
    #[test]
    fn test_simple_reward() {
        let mut b = String::from("x, y\n");
        for i in 0..512 {
            let x: i64 = i * 5000;
            let reward = compute_reward(
                x,
                BigInt::from(0i64),
                BigInt::from(0i64),
                &SIMPLE_TOTAL,
                &BASELINE_TOTAL,
            );

            let x_str = &x.to_string();
            let reward_str = &reward.atto().to_string();
            b.push_str(x_str);
            b.push(',');
            b.push_str(reward_str);
            b.push('\n');
        }

        // compare test output to golden file used for golang tests; file originally located at filecoin-project/specs-actors/actors/builtin/reward/testdata/TestSimpleReward.golden (current link: https://github.com/filecoin-project/specs-actors/blob/d56b240af24517443ce1f8abfbdab7cb22d331f1/actors/builtin/reward/testdata/TestSimpleReward.golden)
        let filename = "testdata/TestSimpleReward.golden";
        let golden_contents =
            fs::read_to_string(filename).expect("Something went wrong reading the file");

        assert_eq!(golden_contents, b);
    }

    // Converted from: https://github.com/filecoin-project/specs-actors/blob/d56b240af24517443ce1f8abfbdab7cb22d331f1/actors/builtin/reward/reward_logic_test.go#L82
    #[test]
    fn test_baseline_reward_growth() {
        fn baseline_in_years(start: StoragePower, x: ChainEpoch) -> StoragePower {
            let mut baseline = start;
            for _ in 0..(x * EPOCHS_IN_YEAR) {
                baseline = baseline_power_from_prev(&baseline);
            }
            baseline
        }

        struct GrowthTestCase {
            start_val: StoragePower,
            err_bound: f64,
        }

        let cases: [GrowthTestCase; 7] = [
            // 1 byte
            GrowthTestCase {
                start_val: StoragePower::from(1i64),
                err_bound: 1.0,
            },
            // GiB
            GrowthTestCase {
                start_val: StoragePower::from(1i64 << 30),
                err_bound: 1e-3,
            },
            // TiB
            GrowthTestCase {
                start_val: StoragePower::from(1i64 << 40),
                err_bound: 1e-6,
            },
            // PiB
            GrowthTestCase {
                start_val: StoragePower::from(1i64 << 50),
                err_bound: 1e-8,
            },
            // EiB
            GrowthTestCase {
                start_val: BASELINE_INITIAL_VALUE.clone(),
                err_bound: 1e-8,
            },
            // ZiB
            GrowthTestCase {
                start_val: StoragePower::from(1u128 << 70),
                err_bound: 1e-8,
            },
            // non power of 2 ~ 1 EiB
            GrowthTestCase {
                start_val: StoragePower::from(513_633_559_722_596_517_u128),
                err_bound: 1e-8,
            },
        ];

        for case in cases {
            let years = 1u32;
            let end = baseline_in_years(case.start_val.clone(), 1);

            // logic from golang test was preserved to enable future testing of more than one year
            let multiplier = BigInt::pow(&BigInt::from(2u32), years);
            let expected = case.start_val * multiplier;
            let diff = &expected - end;

            let perr = BigRational::new(diff, expected)
                .to_f64()
                .expect("BigInt cannot be expressed as a 64bit float");

            assert!(perr < case.err_bound);
        }
    }
}
