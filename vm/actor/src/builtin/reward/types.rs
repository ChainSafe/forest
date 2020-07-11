// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::network::*;
use address::Address;
use encoding::tuple::*;
use num_bigint::{bigint_ser, BigInt, BigUint, ToBigInt};
use num_traits::{Pow, Zero};
use std::ops::Neg;
use vm::TokenAmount;

pub type NetworkTime = BigUint;

/// Number of token units in an abstract "FIL" token.
/// The network works purely in the indivisible token amounts. This constant converts to a fixed decimal with more
/// human-friendly scale.
pub const TOKEN_PRECISION: u64 = 1_000_000_000_000_000_000;

/// Baseline power for the network
pub(super) const BASELINE_POWER: u64 = 1 << 50; // 1PiB for testnet, PARAM_FINISH

/// Fixed-point precision (in bits) used for minting function's input "t"
pub(super) const MINTING_INPUT_FIXED_POINT: usize = 30;

/// Fixed-point precision (in bits) used internally and for output
const MINTING_OUTPUT_FIXED_POINT: u64 = 97;

lazy_static! {
    /// Target reward released to each block winner.
    pub static ref BLOCK_REWARD_TARGET: BigUint = BigUint::from(100u8) * TOKEN_PRECISION;

    pub static ref LAMBDA_NUM: BigInt = BigInt::from(EPOCH_DURATION_SECONDS) * &*LN_TWO_NUM;
    pub static ref LAMBDA_DEN: BigInt = BigInt::from(6*SECONDS_IN_YEAR) * &*LN_TWO_DEN;

    // These numbers are placeholders, but should be in units of attoFIL, 10^-18 FIL
    /// 100M for testnet, PARAM_FINISH
    pub static ref SIMPLE_TOTAL: BigInt = BigInt::from(100).pow(6u8) * BigInt::from(1).pow(18u8);
    /// 900M for testnet, PARAM_FINISH
    pub static ref BASELINE_TOTAL: BigInt = BigInt::from(900).pow(6u8) * BigInt::from(1).pow(18u8);

    // The following are the numerator and denominator of -ln(1/2)=ln(2),
    // represented as a rational with sufficient precision.
    pub static ref LN_TWO_NUM: BigInt = BigInt::from(6_931_471_805_599_453_094_172_321_215u128);
    pub static ref LN_TWO_DEN: BigInt = BigInt::from(10_000_000_000_000_000_000_000_000_000u128);
}

#[derive(Clone, Debug, PartialEq, Serialize_tuple, Deserialize_tuple)]
pub struct AwardBlockRewardParams {
    pub miner: Address,
    #[serde(with = "bigint_ser")]
    pub penalty: TokenAmount,
    #[serde(with = "bigint_ser")]
    pub gas_reward: TokenAmount,
    pub ticket_count: u64,
}

/// Minting Function: Taylor series expansion
///
/// Intent
///   The intent of the following code is to compute the desired fraction of
///   coins that should have been minted at a given epoch according to the
///   simple exponential decay supply. This function is used both directly,
///   to compute simple minting, and indirectly, to compute baseline minting
///   by providing a synthetic "effective network time" instead of an actual
///   epoch number. The prose specification of the simple exponential decay is
///   that the unminted supply should decay exponentially with a half-life of
///   6 years.
fn taylor_series_expansion(lambda_num: &BigInt, lambda_den: &BigInt, t: BigInt) -> BigInt {
    // `numerator_base` is the numerator of the rational representation of (-λt).
    let numerator_base = lambda_num.neg() * t;

    // The denominator of (-λt) is the denominator of λ times the denominator of t,
    // which is a fixed 2^MintingInputFixedPoint. Multiplying by this is a left shift.
    let denominator_base = lambda_den << MINTING_INPUT_FIXED_POINT;

    // `numerator` is the accumulator for numerators of the series terms. The
    // first term is simply (-1)(-λt). To include that factor of (-1), which
    // appears in every term, we introduce this negation into the numerator of
    // the first term. (All terms will be negated by this, because each term is
    // derived from the last by multiplying into it.)
    let mut numerator = numerator_base.clone().neg();
    // `denominator` is the accumulator for denominators of the series terms.
    let mut denominator = denominator_base.clone();

    // `ret` is an _additive_ accumulator for partial sums of the series, and
    // carries a _fixed-point_ representation rather than a rational
    // representation. This just means it has an implicit denominator of
    // 2^(FixedPoint).
    let mut ret = BigInt::zero();

    // The first term computed has order 1; the final term has order 24.
    for n in 1..25 {
        // Multiplying the denominator by `n` on every loop accounts for the
        // `n!` (factorial) in the denominator of the series.
        denominator *= n;

        // Left-shift and divide to convert rational into fixed-point.
        let term = (numerator.clone() << MINTING_OUTPUT_FIXED_POINT) / &denominator;

        // Accumulate the fixed-point result into the return accumulator.
        ret += term;

        // Multiply the rational representation of (-λt) into the term accumulators
        // for the next iteration.  Doing this here in the loop allows us to save a
        // couple bigint operations by initializing numerator and denominator
        // directly instead of multiplying by 1.
        numerator *= &numerator_base;
        denominator *= &denominator_base;

        // If the denominator has grown beyond the necessary precision, then we can
        // truncate it by right-shifting. As long as we right-shift the numerator
        // by the same number of bits, all we have done is lose unnecessary
        // precision that would slow down the next iteration's multiplies.
        let denominator_len = denominator.bits();
        let unnecessary_bits = denominator_len.saturating_sub(MINTING_OUTPUT_FIXED_POINT);

        numerator >>= unnecessary_bits;
        denominator >>= unnecessary_bits;
    }

    ret
}

/// Minting Function Wrapper
///
/// Intent
///   The necessary calling conventions for the function above are unwieldy:
///   the common case is to supply the canonical Lambda, multiply by some other
///   number, and right-shift down by MintingOutputFixedPoint. This convenience
///   wrapper implements those conventions. However, it does NOT implement
///   left-shifting the input by the MintingInputFixedPoint, because baseline
///   minting will actually supply a fractional input.
pub(super) fn minting_function(factor: &BigInt, t: &BigUint) -> BigInt {
    let value = factor
        * taylor_series_expansion(
            &*LAMBDA_NUM,
            &*LAMBDA_DEN,
            t.to_bigint().unwrap_or_default(),
        );

    // This conversion is safe because the minting function should always return a positive value
    value >> MINTING_OUTPUT_FIXED_POINT
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECONDS_IN_YEAR: u64 = 31556925;
    const TEST_EPOCH_DURATION_SECONDS: u64 = 30;
    const MINTING_TEST_VECTOR_PRECISION: u64 = 90;

    // Ported test from specs-actors
    #[test]
    fn minting_function_vectors() {
        let test_lambda_num = BigInt::from(TEST_EPOCH_DURATION_SECONDS) * &*LN_TWO_NUM;
        let test_lambda_den = BigInt::from(6 * SECONDS_IN_YEAR) * &*LN_TWO_DEN;
        let vectors: &[(u64, &str)] = &[
            (1051897, "135060784589637453410950129"),
            (2103794, "255386271058940593613485187"),
            (3155691, "362584098600550296025821387"),
            (4207588, "458086510989070493849325308"),
            (5259485, "543169492437427724953202180"),
            (6311382, "618969815707708523300124489"),
            (7363279, "686500230252085183344830372"),
        ];
        for v in vectors {
            let ts_input = BigInt::from(v.0) << MINTING_INPUT_FIXED_POINT;
            let ts_output = taylor_series_expansion(&test_lambda_num, &test_lambda_den, ts_input);

            let ts_truncated_fractional_part =
                ts_output >> (MINTING_OUTPUT_FIXED_POINT - MINTING_TEST_VECTOR_PRECISION);

            let expected_truncated_fractional_part: BigInt = v.1.parse().unwrap();
            assert_eq!(
                ts_truncated_fractional_part, expected_truncated_fractional_part,
                "failed on input {}, computed: {}, expected: {}",
                v.0, ts_truncated_fractional_part, expected_truncated_fractional_part
            );
        }
    }
}
