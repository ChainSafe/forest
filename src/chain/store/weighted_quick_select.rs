// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::shim::econ::TokenAmount;
use crate::utils::rand::forest_rng;
use num::Zero as _;
use rand::Rng;

/// Performs weighted quick select to find the gas-weighted percentile premium.
///
/// This algorithm selects a value from the premiums array such that the cumulative
/// weight (gas limits) up to that value reaches the target index.
///
/// # Arguments
/// * `premiums` - Array of effective gas premiums
/// * `limits` - Array of gas limits (weights) corresponding to each premium
/// * `target_index` - The target cumulative weight to reach
///
/// # Returns
/// The premium at the target percentile, or zero if not found
pub fn weighted_quick_select(
    mut premiums: Vec<TokenAmount>,
    mut limits: Vec<u64>,
    mut target_index: u64,
) -> TokenAmount {
    loop {
        match (premiums.as_slice(), limits.as_slice()) {
            ([], _) => return TokenAmount::zero(),
            ([premium], [limit]) => {
                return if *limit > target_index {
                    premium.clone()
                } else {
                    TokenAmount::zero()
                };
            }
            _ => {}
        }

        // Choose random pivot
        let pivot_idx = forest_rng().gen_range(0..premiums.len());
        let pivot = premiums
            .get(pivot_idx)
            .expect("pivot_idx is in range")
            .clone();

        // Partition into three groups by premium value relative to pivot
        let (mut more_premiums, mut more_weights, mut more_total_weight) =
            (Vec::new(), Vec::new(), 0u64);
        let mut equal_total_weight = 0u64;
        let (mut less_premiums, mut less_weights, mut less_total_weight) =
            (Vec::new(), Vec::new(), 0u64);

        for (premium, limit) in premiums.into_iter().zip(limits) {
            match premium.cmp(&pivot) {
                std::cmp::Ordering::Greater => {
                    more_total_weight = more_total_weight.saturating_add(limit);
                    more_premiums.push(premium);
                    more_weights.push(limit);
                }
                std::cmp::Ordering::Equal => {
                    equal_total_weight = equal_total_weight.saturating_add(limit);
                }
                std::cmp::Ordering::Less => {
                    less_total_weight = less_total_weight.saturating_add(limit);
                    less_premiums.push(premium);
                    less_weights.push(limit);
                }
            }
        }

        // Determine which partition contains our target
        if target_index < more_total_weight {
            // Target is in the high-premium partition
            premiums = more_premiums;
            limits = more_weights;
        } else if target_index < more_total_weight.saturating_add(equal_total_weight) {
            // Target is in the equal-premium partition
            return pivot;
        } else {
            // Adjust target index by subtracting weights we've passed
            target_index -= more_total_weight.saturating_add(equal_total_weight);

            // Check if target is within the low-premium partition
            if target_index < less_total_weight {
                premiums = less_premiums;
                limits = less_weights;
            } else {
                // Target index exceeds all weights
                return TokenAmount::zero();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_weighted_quick_select_basic() {
        assert_eq!(
            weighted_quick_select(vec![], vec![], 0),
            TokenAmount::zero()
        );

        assert_eq!(
            weighted_quick_select(
                vec![TokenAmount::from_atto(123)],
                vec![5_999_999_999],
                8_000_000_000
            ),
            TokenAmount::zero()
        );

        assert_eq!(
            weighted_quick_select(
                vec![TokenAmount::from_atto(123)],
                vec![8_000_000_001],
                8_000_000_000
            ),
            TokenAmount::from_atto(123)
        );
    }

    #[test]
    fn test_weighted_quick_select_deterministic() {
        // Run multiple times to verify correctness despite randomness
        const TARGET_INDEX: u64 = 7_999_999_999;

        for _ in 0..10 {
            assert_eq!(
                weighted_quick_select(
                    vec![TokenAmount::from_atto(123), TokenAmount::from_atto(100)],
                    vec![5_999_999_999, 2_000_000_000],
                    TARGET_INDEX
                ),
                TokenAmount::zero()
            );

            // Premium value 0 case - returns the premium value 0, not "not found"
            assert_eq!(
                weighted_quick_select(
                    vec![TokenAmount::from_atto(123), TokenAmount::from_atto(0)],
                    vec![5_999_999_999, 2_000_000_001],
                    TARGET_INDEX
                ),
                TokenAmount::from_atto(0)
            );

            assert_eq!(
                weighted_quick_select(
                    vec![TokenAmount::from_atto(123), TokenAmount::from_atto(100)],
                    vec![5_999_999_999, 2_000_000_001],
                    TARGET_INDEX
                ),
                TokenAmount::from_atto(100)
            );

            assert_eq!(
                weighted_quick_select(
                    vec![TokenAmount::from_atto(123), TokenAmount::from_atto(100)],
                    vec![7_999_999_999, 2_000_000_001],
                    TARGET_INDEX
                ),
                TokenAmount::from_atto(100)
            );

            assert_eq!(
                weighted_quick_select(
                    vec![TokenAmount::from_atto(123), TokenAmount::from_atto(100)],
                    vec![8_000_000_000, 2_000_000_000],
                    TARGET_INDEX
                ),
                TokenAmount::from_atto(123)
            );

            assert_eq!(
                weighted_quick_select(
                    vec![TokenAmount::from_atto(123), TokenAmount::from_atto(100)],
                    vec![8_000_000_000, 9_000_000_000],
                    TARGET_INDEX
                ),
                TokenAmount::from_atto(123)
            );

            assert_eq!(
                weighted_quick_select(
                    vec![
                        TokenAmount::from_atto(100),
                        TokenAmount::from_atto(200),
                        TokenAmount::from_atto(300),
                        TokenAmount::from_atto(400),
                        TokenAmount::from_atto(500),
                        TokenAmount::from_atto(600),
                        TokenAmount::from_atto(700),
                    ],
                    vec![
                        4_000_000_000,
                        1_000_000_000,
                        2_000_000_000,
                        1_000_000_000,
                        2_000_000_000,
                        2_000_000_000,
                        3_000_000_000,
                    ],
                    TARGET_INDEX
                ),
                TokenAmount::from_atto(400)
            );
        }
    }

    mod quickcheck_tests {
        use super::*;
        use crate::blocks::BLOCK_MESSAGE_LIMIT;
        use crate::shim::econ::BLOCK_GAS_LIMIT;
        use quickcheck::{Arbitrary, Gen};
        use quickcheck_macros::quickcheck;

        #[derive(Clone, Debug)]
        struct RealisticGasLimits(Vec<u64>);

        impl Arbitrary for RealisticGasLimits {
            fn arbitrary(g: &mut Gen) -> Self {
                let size = usize::arbitrary(g) % BLOCK_MESSAGE_LIMIT;
                let limits: Vec<u64> = (0..size)
                    .map(|_| u64::arbitrary(g) % BLOCK_GAS_LIMIT) // this goes above but that's
                    // fine
                    .collect();
                RealisticGasLimits(limits)
            }
        }

        /// Wrapper for generating matching premiums and limits
        #[derive(Clone, Debug)]
        struct MatchedPremiumsAndLimits {
            premiums: Vec<u64>,
            limits: Vec<u64>,
        }

        impl Arbitrary for MatchedPremiumsAndLimits {
            fn arbitrary(g: &mut Gen) -> Self {
                let limits = RealisticGasLimits::arbitrary(g).0;
                let premiums: Vec<u64> = (0..limits.len()).map(|_| u64::arbitrary(g)).collect();
                MatchedPremiumsAndLimits { premiums, limits }
            }
        }

        #[quickcheck]
        fn prop_result_is_from_input_or_zero(input: MatchedPremiumsAndLimits, target: u64) -> bool {
            let premium_amounts: Vec<TokenAmount> = input
                .premiums
                .iter()
                .map(|&p| TokenAmount::from_atto(p))
                .collect();
            let result =
                weighted_quick_select(premium_amounts.clone(), input.limits.clone(), target);

            // Result must either be zero or one of the input premiums
            result.is_zero() || premium_amounts.iter().any(|p| p == &result)
        }

        #[quickcheck]
        fn prop_empty_input_returns_zero(target: u64) -> bool {
            let result = weighted_quick_select(vec![], vec![], target);
            result.is_zero()
        }

        #[quickcheck]
        fn prop_single_element_behavior(premium: u64, limit: u64, target: u64) -> bool {
            let result =
                weighted_quick_select(vec![TokenAmount::from_atto(premium)], vec![limit], target);

            // If limit > target, should return the premium; otherwise zero
            if limit > target {
                result == TokenAmount::from_atto(premium)
            } else {
                result.is_zero()
            }
        }

        #[quickcheck]
        fn prop_target_beyond_total_weight_returns_zero(input: MatchedPremiumsAndLimits) -> bool {
            if input.limits.is_empty() {
                return true;
            }

            let premium_amounts: Vec<TokenAmount> = input
                .premiums
                .iter()
                .map(|&p| TokenAmount::from_atto(p))
                .collect();

            // Target at or beyond total weight should return zero
            let total_weight: u64 = input.limits.iter().sum();
            let result = weighted_quick_select(premium_amounts, input.limits, total_weight);
            result.is_zero()
        }

        #[quickcheck]
        fn prop_deterministic_result(input: MatchedPremiumsAndLimits, target: u64) -> bool {
            let premium_amounts: Vec<TokenAmount> = input
                .premiums
                .iter()
                .map(|&p| TokenAmount::from_atto(p))
                .collect();

            // Run twice and check results are the same (despite randomness in pivot selection)
            let result1 =
                weighted_quick_select(premium_amounts.clone(), input.limits.clone(), target);
            let result2 = weighted_quick_select(premium_amounts, input.limits, target);

            result1 == result2
        }

        /// Wrapper for two distinct premiums with weights
        #[derive(Clone, Debug)]
        struct OrderedPremiumPair {
            low_premium: u64,
            high_premium: u64,
            weight_low: u64,
            weight_high: u64,
        }

        impl Arbitrary for OrderedPremiumPair {
            fn arbitrary(g: &mut Gen) -> Self {
                const MAX_PREMIUM: u64 = u64::MAX / 2; // Leave room for increment
                let low = u64::arbitrary(g) % MAX_PREMIUM;
                let high = low + (u64::arbitrary(g) % 1000).max(1);
                OrderedPremiumPair {
                    low_premium: low,
                    high_premium: high,
                    weight_low: (u64::arbitrary(g) % BLOCK_GAS_LIMIT).max(1),
                    weight_high: (u64::arbitrary(g) % BLOCK_GAS_LIMIT).max(1),
                }
            }
        }

        #[quickcheck]
        fn prop_result_respects_weight_ordering(pair: OrderedPremiumPair) -> bool {
            // Use target strictly less than weight_high to actually test the behavior.
            // weight_high >= 1 is guaranteed by the Arbitrary impl, so this is safe.
            let target = pair.weight_high - 1;
            let result = weighted_quick_select(
                vec![
                    TokenAmount::from_atto(pair.low_premium),
                    TokenAmount::from_atto(pair.high_premium),
                ],
                vec![pair.weight_low, pair.weight_high],
                target,
            );

            // When target < weight_high, should select the high premium
            // (the high premium's weight alone is sufficient to cover the target)
            result == TokenAmount::from_atto(pair.high_premium)
        }

        #[quickcheck]
        fn prop_equal_premiums_handled_correctly(
            premium: u64,
            limits: RealisticGasLimits,
            target: u64,
        ) -> bool {
            if limits.0.is_empty() {
                return true;
            }

            let premiums = vec![TokenAmount::from_atto(premium); limits.0.len()];
            let total_weight: u64 = limits.0.iter().sum();
            let result = weighted_quick_select(premiums, limits.0, target);

            if target < total_weight {
                result == TokenAmount::from_atto(premium)
            } else {
                result.is_zero()
            }
        }
    }
}
