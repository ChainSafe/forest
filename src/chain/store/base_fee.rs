// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocks::Tipset;
use crate::message::MessageRead;
use crate::prelude::*;
use crate::shim::clock::ChainEpoch;
use crate::shim::econ::{BLOCK_GAS_LIMIT, TokenAmount};
use ahash::HashSet;

use super::weighted_quick_select::weighted_quick_select;

pub const BLOCK_GAS_TARGET_INDEX: u64 = BLOCK_GAS_LIMIT * 80 / 100 - 1;

/// Used in calculating the base fee change.
pub const BLOCK_GAS_TARGET: u64 = BLOCK_GAS_LIMIT / 2;

/// Limits gas base fee change to 12.5% of the change.
pub const BASE_FEE_MAX_CHANGE_DENOM: i64 = 8;

/// Genesis base fee.
pub const PACKING_EFFICIENCY_DENOM: u64 = 5;
pub const PACKING_EFFICIENCY_NUM: u64 = 4;
pub const MINIMUM_BASE_FEE: i64 = 100;

fn compute_next_base_fee(
    base_fee: &TokenAmount,
    gas_limit_used: u64,
    no_of_blocks: usize,
    epoch: ChainEpoch,
    smoke_height: ChainEpoch,
) -> TokenAmount {
    let mut delta: i64 = if epoch > smoke_height {
        (gas_limit_used as i64 / no_of_blocks as i64) - BLOCK_GAS_TARGET as i64
    } else {
        // Yes the denominator and numerator are intentionally flipped here. We are
        // matching go.
        (PACKING_EFFICIENCY_DENOM * gas_limit_used / (no_of_blocks as u64 * PACKING_EFFICIENCY_NUM))
            as i64
            - BLOCK_GAS_TARGET as i64
    };

    // Limit absolute change at the block gas target.
    delta = delta.clamp(-(BLOCK_GAS_TARGET as i64), BLOCK_GAS_TARGET as i64);

    // cap change at 12.5% (BaseFeeMaxChangeDenom) by capping delta
    let change: TokenAmount = (base_fee * delta)
        .div_floor(BLOCK_GAS_TARGET)
        .div_floor(BASE_FEE_MAX_CHANGE_DENOM);
    (base_fee + change).max(TokenAmount::from_atto(MINIMUM_BASE_FEE))
}

pub fn compute_base_fee<DB>(
    db: &DB,
    ts: &Tipset,
    smoke_height: ChainEpoch,
    firehorse_height: ChainEpoch,
) -> Result<TokenAmount, crate::chain::Error>
where
    DB: Blockstore,
{
    if ts.epoch() < firehorse_height {
        return compute_next_base_fee_from_utlilization(db, ts, smoke_height);
    }
    compute_next_base_fee_from_premiums(db, ts)
}

fn compute_next_base_fee_from_premiums<DB>(
    db: &DB,
    ts: &Tipset,
) -> Result<TokenAmount, crate::chain::Error>
where
    DB: Blockstore,
{
    let mut limits = Vec::new();
    let mut premiums = Vec::new();
    let mut seen = HashSet::new();
    let parent_base_fee = &ts.block_headers().first().parent_base_fee;

    for b in ts.block_headers() {
        let (bls_msgs, secp_msgs) = crate::chain::block_messages(db, b)?;
        for m in bls_msgs
            .iter()
            .map(|m| m as &dyn MessageRead)
            .chain(secp_msgs.iter().map(|m| m as &dyn MessageRead))
        {
            if seen.insert((m.from(), m.sequence())) {
                limits.push(m.gas_limit());
                premiums.push(m.effective_gas_premium(parent_base_fee));
            }
        }
    }

    let percentile_premium = weighted_quick_select(premiums, limits, BLOCK_GAS_TARGET_INDEX);
    Ok(compute_next_base_fee_from_premium(
        parent_base_fee,
        percentile_premium,
    ))
}

pub(crate) fn compute_next_base_fee_from_premium(
    base_fee: &TokenAmount,
    percentile_premium: TokenAmount,
) -> TokenAmount {
    let denom = TokenAmount::from_atto(BASE_FEE_MAX_CHANGE_DENOM);
    let max_adj = (base_fee + (&denom - &TokenAmount::from_atto(1))) / denom;
    TokenAmount::from_atto(MINIMUM_BASE_FEE)
        .max(base_fee + (&max_adj).min(&(&percentile_premium - &max_adj)))
}

fn compute_next_base_fee_from_utlilization<DB>(
    db: &DB,
    ts: &Tipset,
    smoke_height: ChainEpoch,
) -> Result<TokenAmount, crate::chain::Error>
where
    DB: Blockstore,
{
    let mut total_limit = 0;
    let mut seen = HashSet::new();

    // Add all unique messages' gas limit to get the total for the Tipset.
    for b in ts.block_headers() {
        let (bls_msgs, secp_msgs) = crate::chain::block_messages(db, b)?;
        for m in bls_msgs.iter().chain(secp_msgs.iter().map(|m| m.message())) {
            if seen.insert(m.cid()) {
                total_limit += m.gas_limit();
            }
        }
    }

    // Compute next base fee based on the current gas limit and parent base fee.
    let parent_base_fee = &ts.block_headers().first().parent_base_fee;
    Ok(compute_next_base_fee(
        parent_base_fee,
        total_limit,
        ts.block_headers().len(),
        ts.epoch(),
        smoke_height,
    ))
}

#[cfg(test)]
mod tests {
    use crate::blocks::RawBlockHeader;
    use crate::blocks::{CachingBlockHeader, Tipset};
    use crate::db::MemoryDB;
    use crate::networks::{ChainConfig, Height};
    use crate::shim::address::Address;
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::no_gas_one_block(100_000_000, 0, 1, 87_500_000, 87_500_000)]
    #[case::no_gas_five_blocks(100_000_000, 0, 5, 87_500_000, 87_500_000)]
    #[case::target_gas_one_block(100_000_000, BLOCK_GAS_TARGET, 1, 103_125_000, 100_000_000)]
    #[case::target_gas_two_blocks(100_000_000, BLOCK_GAS_TARGET * 2, 2, 103_125_000, 100_000_000)]
    #[case::full_gas_two_blocks(100_000_000, BLOCK_GAS_LIMIT * 2, 2, 112_500_000, 112_500_000)]
    #[case::three_quarters_gas_two_blocks(100_000_000, BLOCK_GAS_LIMIT * 15 / 10, 2, 110_937_500, 106_250_000)]
    fn run_base_fee_tests(
        #[case] base_fee: i64,
        #[case] limit_used: u64,
        #[case] no_of_blocks: usize,
        #[case] expected_pre_smoke: i64,
        #[case] expected_post_smoke: i64,
    ) {
        let smoke_height = ChainConfig::default().epoch(Height::Smoke);
        let base_fee = TokenAmount::from_atto(base_fee);

        let output = compute_next_base_fee(
            &base_fee,
            limit_used,
            no_of_blocks,
            smoke_height - 1,
            smoke_height,
        );
        assert_eq!(TokenAmount::from_atto(expected_pre_smoke), output);

        let output = compute_next_base_fee(
            &base_fee,
            limit_used,
            no_of_blocks,
            smoke_height + 1,
            smoke_height,
        );
        assert_eq!(TokenAmount::from_atto(expected_post_smoke), output);
    }

    #[test]
    fn compute_base_fee_shouldnt_panic_on_bad_input() {
        let blockstore = MemoryDB::default();
        let h0 = CachingBlockHeader::new(RawBlockHeader {
            miner_address: Address::new_id(0),
            ..Default::default()
        });
        let ts = Tipset::from(h0);
        let smoke_height = ChainConfig::default().epoch(Height::Smoke);
        let firehorse_height = ChainConfig::default().epoch(Height::FireHorse);
        assert!(compute_base_fee(&blockstore, &ts, smoke_height, firehorse_height).is_err());
    }

    // Test cases from the FIP-0115
    // <https://github.com/filecoin-project/FIPs/blob/b84b89a34ccb3d239493392a7867d6b082193b38/FIPS/fip-0115.md#basefee_next>
    #[rstest]
    #[case(100, 0, 100)]
    #[case(100, 13, 100)]
    #[case(100, 14, 101)]
    #[case(100, 26, 113)]
    #[case(801, 0, 700)]
    #[case(801, 20, 720)]
    #[case(801, 40, 740)]
    #[case(801, 60, 760)]
    #[case(801, 80, 780)]
    #[case(801, 100, 800)]
    #[case(801, 120, 820)]
    #[case(801, 140, 840)]
    #[case(801, 160, 860)]
    #[case(801, 180, 880)]
    #[case(801, 200, 900)]
    #[case(801, 201, 901)]
    #[case(808, 0, 707)]
    #[case(808, 1, 708)]
    #[case(808, 201, 908)]
    #[case(808, 202, 909)]
    #[case(808, 203, 909)]
    fn test_next_base_fee_from_premium(
        #[case] base_fee: u64,
        #[case] premium: u64,
        #[case] expected: u64,
    ) {
        let result = compute_next_base_fee_from_premium(
            &TokenAmount::from_atto(base_fee),
            TokenAmount::from_atto(premium),
        );
        assert_eq!(result, TokenAmount::from_atto(expected));
    }

    mod quickcheck_tests {
        use super::*;
        use num::Zero;
        use quickcheck_macros::quickcheck;

        #[quickcheck]
        fn prop_next_base_fee_never_below_minimum(base_fee: TokenAmount, premium: TokenAmount) {
            let result = compute_next_base_fee_from_premium(&base_fee, premium);
            assert!(result >= TokenAmount::from_atto(MINIMUM_BASE_FEE));
        }

        #[quickcheck]
        fn prop_next_base_fee_change_bounded(base_fee: TokenAmount, premium: TokenAmount) -> bool {
            if base_fee.is_zero() {
                return true;
            }
            let min_fee = TokenAmount::from_atto(MINIMUM_BASE_FEE);
            let result = compute_next_base_fee_from_premium(&base_fee, premium);

            // maxAdj = ceil(base_fee / 8)
            let denom = TokenAmount::from_atto(BASE_FEE_MAX_CHANGE_DENOM);
            let max_adj = (&base_fee + &denom - TokenAmount::from_atto(1)) / &denom;

            // Result should be within [max(MIN, base - max_adj), max(MIN, base + max_adj)]
            let lower = min_fee.clone().max(&base_fee - &max_adj);
            let upper = min_fee.max(&base_fee + &max_adj);

            result >= lower && result <= upper
        }

        #[quickcheck]
        fn prop_next_base_fee_monotonic_in_premium(
            base_fee: TokenAmount,
            premium1: TokenAmount,
            premium2: TokenAmount,
        ) {
            let result1 = compute_next_base_fee_from_premium(&base_fee, premium1.clone());
            let result2 = compute_next_base_fee_from_premium(&base_fee, premium2.clone());

            // Higher premium should give higher or equal result
            if premium1 <= premium2 {
                assert!(result1 <= result2);
            } else {
                assert!(result1 >= result2);
            }
        }

        #[quickcheck]
        fn prop_zero_premium_decreases_or_maintains_base_fee(base_fee: TokenAmount) {
            let min_fee = TokenAmount::from_atto(MINIMUM_BASE_FEE);
            let result = compute_next_base_fee_from_premium(&base_fee, TokenAmount::zero());

            // With zero premium, base fee should decrease (or stay at minimum)
            assert!(result <= base_fee || result == min_fee);
        }
    }
}
