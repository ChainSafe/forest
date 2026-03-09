// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocks::Tipset;
use crate::message::Message;
use crate::shim::clock::ChainEpoch;
use crate::shim::econ::{BLOCK_GAS_LIMIT, TokenAmount};
use ahash::{HashSet, HashSetExt};
use fvm_ipld_blockstore::Blockstore;

use super::weighted_quick_select::weighted_quick_select;

/// Placeholder for the FIP-0115 activation height.
/// Replace with the actual network upgrade height once finalized.
/// Tracked in <https://github.com/ChainSafe/forest/issues/6704>
pub const PLACEHOLDER_NEXT_UPGRADE_HEIGHT: ChainEpoch = ChainEpoch::MAX;

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
    next_upgrade_height: ChainEpoch,
) -> Result<TokenAmount, crate::chain::Error>
where
    DB: Blockstore,
{
    // FIP-0115: https://github.com/filecoin-project/FIPs/pull/1233
    if ts.epoch() >= next_upgrade_height {
        return compute_next_base_fee_from_premiums(db, ts);
    }

    compute_next_base_fee_from_utlilization(db, ts, smoke_height)
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
            .map(|m| m as &dyn Message)
            .chain(secp_msgs.iter().map(|m| m as &dyn Message))
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
        for m in bls_msgs.iter().chain(secp_msgs.iter().map(|m| &m.message)) {
            if seen.insert(m.cid()) {
                total_limit += m.gas_limit;
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

    use super::*;

    fn construct_tests() -> Vec<(i64, u64, usize, i64, i64)> {
        // (base_fee, limit_used, no_of_blocks, output)
        vec![
            (100_000_000, 0, 1, 87_500_000, 87_500_000),
            (100_000_000, 0, 5, 87_500_000, 87_500_000),
            (100_000_000, BLOCK_GAS_TARGET, 1, 103_125_000, 100_000_000),
            (
                100_000_000,
                BLOCK_GAS_TARGET * 2,
                2,
                103_125_000,
                100_000_000,
            ),
            (
                100_000_000,
                BLOCK_GAS_LIMIT * 2,
                2,
                112_500_000,
                112_500_000,
            ),
            (
                100_000_000,
                BLOCK_GAS_LIMIT * 15 / 10,
                2,
                110_937_500,
                106_250_000,
            ),
        ]
    }

    #[test]
    fn run_base_fee_tests() {
        let smoke_height = ChainConfig::default().epoch(Height::Smoke);
        let cases = construct_tests();

        for case in cases {
            // Pre smoke
            let output = compute_next_base_fee(
                &TokenAmount::from_atto(case.0),
                case.1,
                case.2,
                smoke_height - 1,
                smoke_height,
            );
            assert_eq!(TokenAmount::from_atto(case.3), output);

            // Post smoke
            let output = compute_next_base_fee(
                &TokenAmount::from_atto(case.0),
                case.1,
                case.2,
                smoke_height + 1,
                smoke_height,
            );
            assert_eq!(TokenAmount::from_atto(case.4), output);
        }
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
        assert!(
            compute_base_fee(
                &blockstore,
                &ts,
                smoke_height,
                PLACEHOLDER_NEXT_UPGRADE_HEIGHT
            )
            .is_err()
        );
    }

    #[test]
    fn test_next_base_fee_from_premium() {
        // Test cases from Lotus PR #13531
        let test_cases = vec![
            (100, 0, 100),
            (100, 13, 100),
            (100, 14, 101),
            (100, 26, 113),
            (801, 0, 700),
            (801, 20, 720),
            (801, 40, 740),
            (801, 60, 760),
            (801, 80, 780),
            (801, 100, 800),
            (801, 120, 820),
            (801, 140, 840),
            (801, 160, 860),
            (801, 180, 880),
            (801, 200, 900),
            (801, 201, 901),
            (808, 0, 707),
            (808, 1, 708),
            (808, 201, 908),
            (808, 202, 909),
            (808, 203, 909),
        ];

        for (base_fee, premium_p, expected) in test_cases {
            let base_fee = TokenAmount::from_atto(base_fee);
            let premium = TokenAmount::from_atto(premium_p);

            let result = compute_next_base_fee_from_premium(&base_fee, premium);

            assert_eq!(
                result,
                TokenAmount::from_atto(expected),
                "Failed for base_fee={}, premium_p={}",
                base_fee.atto(),
                premium_p
            );
        }
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
