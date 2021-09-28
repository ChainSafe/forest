// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use clock::ChainEpoch;
use fil_types::StoragePower;
use forest_actor::math::{poly_parse, PRECISION};
use forest_actor::smooth::extrapolated_cum_sum_of_ratio as ecsor;
use forest_actor::smooth::*;
use forest_actor::EPOCHS_IN_DAY;
use num_bigint::{BigInt, Integer};
use num_traits::sign::Signed;

const ERR_BOUND: u64 = 350;

// Millionths of difference between val1 and val2
// (val1 - val2) / val1 * 1e6
// all inputs Q.128, output Q.0
fn per_million_error(val_1: &BigInt, val_2: &BigInt) -> BigInt {
    let diff = (val_1 - val_2) << PRECISION;

    let ratio = diff.div_floor(&val_1);
    let million = BigInt::from(1_000_000) << PRECISION;

    let diff_per_million = (ratio * million).abs();

    diff_per_million >> (2 * PRECISION)
}

fn iterative_cum_sum_of_ratio(
    num: &FilterEstimate,
    denom: &FilterEstimate,
    t0: ChainEpoch,
    delta: ChainEpoch,
) -> BigInt {
    let mut ratio = BigInt::from(0u8);

    for i in 0..delta {
        let num_epsilon = num.extrapolate(t0 + i); // Q.256
        let denom_epsilon = denom.extrapolate(t0 + i) >> PRECISION; // Q.256
        let mut epsilon = num_epsilon.div_floor(&denom_epsilon); // Q.256 / Q.128 => Q.128

        if i != 0 && i != delta - 1 {
            epsilon *= 2; // Q.128 * Q.0 => Q.128
        }
        ratio += epsilon;
    }

    ratio.div_floor(&BigInt::from(2))
}

fn assert_err_bound(
    num: &FilterEstimate,
    denom: &FilterEstimate,
    delta: ChainEpoch,
    t0: ChainEpoch,
    err_bound: BigInt,
) {
    let analytic = ecsor(delta, t0, num, denom);
    let iterative = iterative_cum_sum_of_ratio(num, denom, t0, delta);
    let actual_err = per_million_error(&analytic, &iterative);
    assert!(
        actual_err < err_bound,
        "Values are {} and {}",
        actual_err,
        err_bound
    );
}

// Returns an estimate with position val and velocity 0
pub fn testing_constant_estimate(val: BigInt) -> FilterEstimate {
    FilterEstimate::new(val, BigInt::from(0u8))
}

// Returns and estimate with postion x and velocity v
pub fn testing_estimate(x: BigInt, v: BigInt) -> FilterEstimate {
    FilterEstimate::new(x, v)
}

#[test]
fn test_natural_log() {
    let ln_inputs: Vec<BigInt> = poly_parse(&[
        "340282366920938463463374607431768211456", // Q.128 format of 1
        "924990000000000000000000000000000000000", // Q.128 format of e (rounded up in 5th decimal place to handle truncation)
        "34028236692093846346337460743176821145600000000000000000000", // Q.128 format of 100e18
        "6805647338418769269267492148635364229120000000000000000000000", // Q.128 format of 2e22
        "204169000000000000000000000000000000",    // Q.128 format of 0.0006
        "34028236692093846346337460743",           // Q.128 format of 1e-10
    ])
    .unwrap();

    let expected_ln_outputs: Vec<BigInt> = poly_parse(&[
        "0",                                         // Q.128 format of 0 = ln(1)
        "340282366920938463463374607431768211456",   // Q.128 format of 1 = ln(e)
        "15670582109617661336106769654068947397831", // Q.128 format of 46.051... = ln(100e18)
        "17473506083804940763855390762239996622013", // Q.128 format of  51.35... = ln(2e22)
        "-2524410000000000000000000000000000000000", // Q.128 format of -7.41.. = ln(0.0006)
        "-7835291054808830668053384827034473698915", // Q.128 format of -23.02.. = ln(1e-10)
    ])
    .unwrap();

    assert_eq!(ln_inputs.len(), expected_ln_outputs.len());
    let num_inputs = ln_inputs.len();

    for i in 0..num_inputs {
        let z = &ln_inputs[i];
        let ln_of_z = ln(z);
        let expected_z = &expected_ln_outputs[i];
        assert_eq!(expected_z >> PRECISION, ln_of_z >> PRECISION);
    }
}

#[test]
fn constant_estimate() {
    let num_estimate = testing_constant_estimate(BigInt::from(4_000_000));
    let denom_estimate = testing_constant_estimate(BigInt::from(1));

    // 4e6/1 over 1000 epochs should give us 4e9
    let csr_1 = ecsor(1000, 0, &num_estimate, &denom_estimate) >> PRECISION;
    assert_eq!(BigInt::from(4 * 10_i64.pow(9)), csr_1);

    // if we change t0 nothing should change because velocity is 0
    let csr_2 = ecsor(1000, 10_i64.pow(15), &num_estimate, &denom_estimate) >> PRECISION;

    assert_eq!(csr_1, csr_2);

    // 1e12 / 200e12 for 100 epochs should give ratio of 1/2
    let num_estimate = testing_constant_estimate(BigInt::from(10_i64.pow(12)));
    let denom_estimate = testing_constant_estimate(BigInt::from(200 * 10_i64.pow(12)));
    let csr_frac = ecsor(100, 0, &num_estimate, &denom_estimate);

    // If we didn't return Q.128 we'd just get zero
    assert_eq!(BigInt::from(0u8), &csr_frac >> PRECISION);

    // multiply by 10k and we'll get 5k
    // note: this is a bit sensative to input, lots of numbers approach from below
    // (...99999) and so truncating division takes us off by one
    let product = csr_frac * (BigInt::from(10_000) << PRECISION); // Q.256
    assert_eq!(BigInt::from(5000), product >> (2 * PRECISION));
}

#[test]
fn both_positive_velocity() {
    let num_estimate = testing_estimate(BigInt::from(111), BigInt::from(12));
    let denom_estimate = testing_estimate(BigInt::from(3456), BigInt::from(8));
    assert_err_bound(
        &num_estimate,
        &denom_estimate,
        10_000,
        0,
        BigInt::from(ERR_BOUND),
    );
}

#[test]
fn flipped_signs() {
    let num_estimate = testing_estimate(BigInt::from(1_000_000), BigInt::from(-100));
    let denom_estimate = testing_estimate(BigInt::from(70_000), BigInt::from(1000));
    assert_err_bound(
        &num_estimate,
        &denom_estimate,
        100_000,
        0,
        BigInt::from(ERR_BOUND),
    );
}

#[test]
fn values_in_range() {
    let tens_of_fil = BigInt::from(50 * 10_i128.pow(18));
    let one_fil_per_sec = BigInt::from(25);
    let four_fil_per_second = BigInt::from(100);

    let slow_money = testing_estimate(tens_of_fil.clone(), one_fil_per_sec);
    let fast_money = testing_estimate(tens_of_fil, four_fil_per_second);

    let tens_of_ei_bs = StoragePower::from(10_i128.pow(19));
    let thousands_of_ei_bs = StoragePower::from(2 * 10_i128.pow(22));

    let one_byte_per_epoch_velocity = BigInt::from(1);
    let ten_pi_bs_per_day_velocity =
        BigInt::from(10 * 2_i128.pow(50)) / BigInt::from(EPOCHS_IN_DAY);
    let one_ei_bs_per_day_velocity = BigInt::from(2_i128.pow(60)) / BigInt::from(EPOCHS_IN_DAY);

    let delta = EPOCHS_IN_DAY;
    let t0 = 0;
    let err_bound = BigInt::from(ERR_BOUND);

    let test_cases: Vec<(StoragePower, BigInt)> = vec![
        (tens_of_ei_bs.clone(), one_byte_per_epoch_velocity.clone()),
        (tens_of_ei_bs.clone(), ten_pi_bs_per_day_velocity.clone()),
        (tens_of_ei_bs, one_ei_bs_per_day_velocity.clone()),
        (thousands_of_ei_bs.clone(), one_byte_per_epoch_velocity),
        (thousands_of_ei_bs.clone(), ten_pi_bs_per_day_velocity),
        (thousands_of_ei_bs, one_ei_bs_per_day_velocity),
    ];

    for test_case in test_cases {
        let power = testing_estimate(test_case.0, test_case.1);
        assert_err_bound(&slow_money, &power, delta, t0, err_bound.clone());
        assert_err_bound(&fast_money, &power, delta, t0, err_bound.clone());
    }
}
