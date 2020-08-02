// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[macro_use]
extern crate lazy_static;

use num_bigint::BigInt;

mod alpha_beta_filter;
mod constants;
pub use alpha_beta_filter::*;
use clock::ChainEpoch;
use constants::*;
use forest_math::{poly_val, PRECISION};
use num_traits::sign::Signed;

fn get_bit_len(z: &BigInt) -> usize {
    z.abs().to_radix_le(2).1.len()
}

pub fn extrapolated_cum_sum_of_ratio(
    delta: ChainEpoch,
    relative_start: ChainEpoch,
    est_num: &FilterEstimate,
    est_denom: &FilterEstimate,
) -> BigInt {
    let delta_t = BigInt::from(delta) << PRECISION;
    let t0 = BigInt::from(relative_start) << PRECISION;

    let pos_1 = &est_num.pos;
    let pos_2 = &est_denom.pos;
    let velo_1 = &est_num.velo;
    let velo_2 = &est_denom.velo;

    let squared_velo_2 = (velo_2 * velo_2) >> PRECISION;

    if squared_velo_2 >= get_cum_sum_ratio_epsilon() {
        let mut x2a = ((velo_2 * t0) >> PRECISION) + pos_2;
        let mut x2b = ((velo_2 * &delta_t) >> PRECISION) + &x2a;
        x2a = ln(&x2a);
        x2b = ln(&x2b);

        let m1 = ((&x2b - &x2a) * pos_1 * velo_2) >> PRECISION;

        let m2_l = (&x2a - &x2b) * pos_2;
        let m2_r = velo_2 * &delta_t;
        let m2 = ((m2_l + m2_r) * velo_1) >> PRECISION;

        return (m2 + m1) / squared_velo_2;
    }

    let half_delta = &delta_t >> 1;
    let mut x1m = velo_1 * (t0 + half_delta);
    x1m = (x1m >> PRECISION) + pos_1;

    (x1m * delta_t) / pos_2
}

pub fn ln(z: &BigInt) -> BigInt {
    let k: i64 = get_bit_len(z) as i64 - 1 - PRECISION as i64;

    let x: BigInt = if k > 0 { z >> k } else { z << k.abs() };

    BigInt::from(k) * get_ln2() + ln_between_one_and_two(x)
}

fn ln_between_one_and_two(x: BigInt) -> BigInt {
    let num = poly_val(&get_ln_num_coef(), &x) << PRECISION;
    let denom = poly_val(&get_ln_denom_coef(), &x);
    num / denom
}

// Returns an estimate with position val and velocity 0
pub fn testing_constant_estimate(val: BigInt) -> FilterEstimate {
    FilterEstimate::new(val, BigInt::from(0u8))
}

// Returns and estimate with postion x and velocity v
pub fn testing_estimate(x: BigInt, v: BigInt) -> FilterEstimate {
    FilterEstimate::new(x, v)
}
