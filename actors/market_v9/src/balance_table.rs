// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_hamt::Error as HamtError;
use fvm_shared::address::Address;
use fvm_shared::econ::TokenAmount;
use num_traits::Zero;

use fil_actors_runtime_v9::{make_empty_map, make_map_with_root_and_bitwidth, Map};

pub const BALANCE_TABLE_BITWIDTH: u32 = 6;

/// Balance table which handles getting and updating token balances specifically
pub struct BalanceTable<'a, BS>(pub Map<'a, BS, TokenAmount>);

impl<'a, BS> BalanceTable<'a, BS>
where
    BS: Blockstore,
{
    /// Initializes a new empty balance table
    pub fn new(bs: &'a BS) -> Self {
        Self(make_empty_map(bs, BALANCE_TABLE_BITWIDTH))
    }

    /// Initializes a balance table from a root Cid
    pub fn from_root(bs: &'a BS, cid: &Cid) -> Result<Self, HamtError> {
        Ok(Self(make_map_with_root_and_bitwidth(
            cid,
            bs,
            BALANCE_TABLE_BITWIDTH,
        )?))
    }

    /// Retrieve root from balance table
    pub fn root(&mut self) -> Result<Cid, HamtError> {
        self.0.flush()
    }

    /// Gets token amount for given address in balance table
    pub fn get(&self, key: &Address) -> Result<TokenAmount, HamtError> {
        if let Some(v) = self.0.get(&key.to_bytes())? {
            Ok(v.clone())
        } else {
            Ok(TokenAmount::zero())
        }
    }

    /// Adds token amount to previously initialized account.
    pub fn add(&mut self, key: &Address, value: &TokenAmount) -> Result<(), HamtError> {
        let prev = self.get(key)?;
        let sum = &prev + value;
        if sum.is_negative() {
            Err(format!("New balance in table cannot be negative: {}", sum).into())
        } else if sum.is_zero() && !prev.is_zero() {
            self.0.delete(&key.to_bytes())?;
            Ok(())
        } else {
            self.0.set(key.to_bytes().into(), sum)?;
            Ok(())
        }
    }

    /// Subtracts up to the specified amount from a balance, without reducing the balance
    /// below some minimum.
    /// Returns the amount subtracted (always positive or zero).
    pub fn subtract_with_minimum(
        &mut self,
        key: &Address,
        req: &TokenAmount,
        floor: &TokenAmount,
    ) -> Result<TokenAmount, HamtError> {
        let prev = self.get(key)?;
        let available = std::cmp::max(TokenAmount::zero(), prev - floor);
        let sub: TokenAmount = std::cmp::min(&available, req).clone();

        if sub.is_positive() {
            self.add(key, &-sub.clone())?;
        }

        Ok(sub)
    }

    /// Subtracts value from a balance, and errors if full amount was not substracted.
    pub fn must_subtract(&mut self, key: &Address, req: &TokenAmount) -> Result<(), HamtError> {
        let prev = self.get(key)?;

        if req > &prev {
            Err("couldn't subtract the requested amount".into())
        } else {
            self.add(key, &-req)
        }
    }

    /// Returns total balance held by this balance table
    #[allow(dead_code)]
    pub fn total(&self) -> Result<TokenAmount, HamtError> {
        let mut total = TokenAmount::zero();

        self.0.for_each(|_, v: &TokenAmount| {
            total += v;
            Ok(())
        })?;

        Ok(total)
    }
}

#[cfg(test)]
mod tests {
    use fvm_ipld_blockstore::MemoryBlockstore;
    use fvm_shared::address::Address;
    use fvm_shared::econ::TokenAmount;

    use crate::balance_table::BalanceTable;

    #[test]
    fn total() {
        let addr1 = Address::new_id(100);
        let addr2 = Address::new_id(101);
        let store = MemoryBlockstore::default();
        let mut bt = BalanceTable::new(&store);

        assert!(bt.total().unwrap().is_zero());

        struct TotalTestCase<'a> {
            amount: u64,
            addr: &'a Address,
            total: u64,
        }
        let cases = [
            TotalTestCase {
                amount: 10,
                addr: &addr1,
                total: 10,
            },
            TotalTestCase {
                amount: 20,
                addr: &addr1,
                total: 30,
            },
            TotalTestCase {
                amount: 40,
                addr: &addr2,
                total: 70,
            },
            TotalTestCase {
                amount: 50,
                addr: &addr2,
                total: 120,
            },
        ];

        for t in cases.iter() {
            bt.add(t.addr, &TokenAmount::from_atto(t.amount)).unwrap();

            assert_eq!(bt.total().unwrap(), TokenAmount::from_atto(t.total));
        }
    }

    #[test]
    fn balance_subtracts() {
        let addr = Address::new_id(100);
        let store = MemoryBlockstore::default();
        let mut bt = BalanceTable::new(&store);

        bt.add(&addr, &TokenAmount::from_atto(80u8)).unwrap();
        assert_eq!(bt.get(&addr).unwrap(), TokenAmount::from_atto(80u8));
        // Test subtracting past minimum only subtracts correct amount
        assert_eq!(
            bt.subtract_with_minimum(
                &addr,
                &TokenAmount::from_atto(20u8),
                &TokenAmount::from_atto(70u8)
            )
            .unwrap(),
            TokenAmount::from_atto(10u8)
        );
        assert_eq!(bt.get(&addr).unwrap(), TokenAmount::from_atto(70u8));

        // Test subtracting to limit
        assert_eq!(
            bt.subtract_with_minimum(
                &addr,
                &TokenAmount::from_atto(10u8),
                &TokenAmount::from_atto(60u8)
            )
            .unwrap(),
            TokenAmount::from_atto(10u8)
        );
        assert_eq!(bt.get(&addr).unwrap(), TokenAmount::from_atto(60u8));

        // Test must subtract success
        bt.must_subtract(&addr, &TokenAmount::from_atto(10u8))
            .unwrap();
        assert_eq!(bt.get(&addr).unwrap(), TokenAmount::from_atto(50u8));

        // Test subtracting more than available
        assert!(bt
            .must_subtract(&addr, &TokenAmount::from_atto(100u8))
            .is_err());
    }
}
