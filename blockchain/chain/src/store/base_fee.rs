// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use blocks::Tipset;
use encoding::Cbor;
use ipld_blockstore::BlockStore;
use message::Message;
use num_bigint::{BigInt, Integer};
use std::collections::HashSet;
use types::BLOCK_GAS_LIMIT;

pub const BLOCK_GAS_TARGET: i64 = (BLOCK_GAS_LIMIT / 2) as i64;
pub const BASE_FEE_MAX_CHANGE_DENOM: i64 = 8; // 12.5%;
pub const INITIAL_BASE_FEE: i64 = 100000000; // Genesis base fee
pub const PACKING_EFFICIENCY_DENOM: i64 = 5;
pub const PACKING_EFFICIENCY_NUM: i64 = 4;

lazy_static! {
    /// Cbor bytes of an empty array serialized.
    pub static ref MINIMUM_BASE_FEE: BigInt = 100.into();

    /// These statics are just to avoid allocations for division.
    static ref BLOCK_GAS_TARGET_BIG: BigInt = BigInt::from(BLOCK_GAS_TARGET);
    static ref BASE_FEE_MAX_CHANGE_DENOM_BIG: BigInt = BigInt::from(BASE_FEE_MAX_CHANGE_DENOM);
}

fn compute_next_base_fee(base_fee: &BigInt, gas_limit_used: i64, no_of_blocks: usize) -> BigInt {
    let mut delta: i64 = (PACKING_EFFICIENCY_DENOM * gas_limit_used
        / (no_of_blocks as i64 * PACKING_EFFICIENCY_NUM))
        - BLOCK_GAS_TARGET;
    // cap change at 12.5% (BaseFeeMaxChangeDenom) by capping delta
    if delta.abs() > BLOCK_GAS_TARGET {
        delta = if delta.is_positive() {
            BLOCK_GAS_TARGET
        } else {
            -BLOCK_GAS_TARGET
        };
    }

    let change: BigInt = (base_fee * delta)
        .div_floor(&BLOCK_GAS_TARGET_BIG)
        .div_floor(&BASE_FEE_MAX_CHANGE_DENOM_BIG);
    let mut next_base_fee = base_fee + change;
    if next_base_fee < *MINIMUM_BASE_FEE {
        next_base_fee = MINIMUM_BASE_FEE.clone();
    }
    next_base_fee
}

pub fn compute_base_fee<DB>(db: &DB, ts: &Tipset) -> Result<BigInt, crate::Error>
where
    DB: BlockStore,
{
    let mut total_limit = 0;
    let mut seen = HashSet::new();

    for b in ts.blocks() {
        let (msg1, msg2) = crate::block_messages(db, &b)?;
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
    let parent_base_fee = ts.blocks()[0].parent_base_fee();
    Ok(compute_next_base_fee(
        parent_base_fee,
        total_limit,
        ts.blocks().len(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn construct_tests() -> Vec<(i64, i64, usize, i64)> {
        // (base_fee, limit_used, no_of_blocks, output)
        let mut cases = Vec::new();
        cases.push((100_000_000, 0, 1, 87_500_000));
        cases.push((100_000_000, 0, 5, 87_500_000));
        cases.push((100_000_000, BLOCK_GAS_TARGET, 1, 103_125_000));
        cases.push((100_000_000, BLOCK_GAS_TARGET * 2, 2, 103_125_000));
        cases.push((100_000_000, BLOCK_GAS_LIMIT * 2, 2, 112_500_000));
        cases.push((100_000_000, BLOCK_GAS_LIMIT * 15 / 10, 2, 110_937_500));
        cases
    }

    #[test]
    fn run_base_fee_tests() {
        let cases = construct_tests();

        for case in cases {
            let output = compute_next_base_fee(&case.0.into(), case.1, case.2);
            assert_eq!(BigInt::from(case.3), output);
        }
    }
}
