// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocks::Tipset;
use crate::message::Message;
use crate::shim::clock::ChainEpoch;
use crate::shim::econ::TokenAmount;
use ahash::{HashSet, HashSetExt};
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::Cbor;
use fvm_shared3::BLOCK_GAS_LIMIT;

/// Used in calculating the base fee change.
pub const BLOCK_GAS_TARGET: u64 = BLOCK_GAS_LIMIT / 2;

/// Limits gas base fee change to 12.5% of the change.
pub const BASE_FEE_MAX_CHANGE_DENOM: i64 = 8;

/// Genesis base fee.
pub const INITIAL_BASE_FEE: i64 = 100000000;
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
    if delta.abs() > BLOCK_GAS_TARGET as i64 {
        delta = if delta.is_positive() {
            BLOCK_GAS_TARGET as i64
        } else {
            -(BLOCK_GAS_TARGET as i64)
        };
    }

    // cap change at 12.5% (BaseFeeMaxChangeDenom) by capping delta
    let change: TokenAmount = (base_fee * delta)
        .div_floor(BLOCK_GAS_TARGET)
        .div_floor(BASE_FEE_MAX_CHANGE_DENOM);
    let mut next_base_fee = base_fee + change;
    if next_base_fee.atto() < &MINIMUM_BASE_FEE.into() {
        next_base_fee = TokenAmount::from_atto(MINIMUM_BASE_FEE);
    }
    next_base_fee
}

pub fn compute_base_fee<DB>(
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
    for b in ts.blocks() {
        let (msg1, msg2) = crate::chain::block_messages(db, b)?;
        for m in msg1 {
            let m_cid = m.cid()?;
            if !seen.contains(&m_cid) {
                total_limit += m.gas_limit();
                seen.insert(m_cid);
            }
        }
        for m in msg2 {
            let m_cid = m.cid()?;
            if !seen.contains(&m_cid) {
                total_limit += m.gas_limit();
                seen.insert(m_cid);
            }
        }
    }

    // Compute next base fee based on the current gas limit and parent base fee.
    let parent_base_fee = ts.blocks()[0].parent_base_fee();
    Ok(compute_next_base_fee(
        parent_base_fee,
        total_limit,
        ts.blocks().len(),
        ts.epoch(),
        smoke_height,
    ))
}

#[cfg(test)]
mod tests {
    use crate::networks::{ChainConfig, Height};

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
}
