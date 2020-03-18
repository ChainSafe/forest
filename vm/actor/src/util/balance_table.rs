// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::HAMT_BIT_WIDTH;
use address::Address;
use cid::Cid;
use ipld_blockstore::BlockStore;
use ipld_hamt::{Error, Hamt};
use num_traits::CheckedSub;
use vm::TokenAmount;

/// Balance table which handles getting and updating token balances specifically
pub struct BalanceTable<'a, BS>(Hamt<'a, String, BS>);
impl<'a, BS> BalanceTable<'a, BS>
where
    BS: BlockStore,
{
    /// Initializes a new empty balance table
    pub fn new_empty(bs: &'a BS) -> Self {
        Self(Hamt::new_with_bit_width(bs, HAMT_BIT_WIDTH))
    }

    /// Initializes a balance table from a root Cid
    pub fn from_root(bs: &'a BS, cid: &Cid) -> Result<Self, Error> {
        Ok(Self(Hamt::load_with_bit_width(cid, bs, HAMT_BIT_WIDTH)?))
    }

    /// Retrieve root from balance table
    #[inline]
    pub fn root(&mut self) -> Result<Cid, Error> {
        self.0.flush()
    }

    /// Gets token amount for given address in balance table
    #[inline]
    pub fn get(&self, key: &Address) -> Result<TokenAmount, String> {
        Ok(self
            .0
            .get(&key.hash_key())?
            // TODO investigate whether it's worth it to cache root to give better error details
            .ok_or("no key {} in map root")?)
    }

    /// Checks if a balance for an address exists
    #[inline]
    pub fn has(&self, key: &Address) -> Result<bool, Error> {
        match self.0.get::<_, TokenAmount>(&key.hash_key())? {
            Some(_) => Ok(true),
            None => Ok(false),
        }
    }

    /// Sets the balance for the address, overwriting previous value
    #[inline]
    pub fn set(&mut self, key: &Address, value: TokenAmount) -> Result<(), Error> {
        self.0.set(key.hash_key(), value)
    }

    /// Adds token amount to previously initialized account.
    pub fn add(&mut self, key: &Address, value: TokenAmount) -> Result<(), String> {
        let prev = self.get(key)?;
        Ok(self.0.set(key.hash_key(), prev + value)?)
    }

    /// Adds an amount to a balance. Creates entry if not exists
    pub fn add_create(&mut self, key: &Address, value: TokenAmount) -> Result<(), String> {
        let new_val = match self.0.get::<_, TokenAmount>(&key.hash_key())? {
            Some(v) => v + value,
            None => value,
        };
        Ok(self.0.set(key.hash_key(), new_val)?)
    }

    /// Subtracts up to the specified amount from a balance, without reducing the balance
    /// below some minimum.
    /// Returns the amount subtracted (always positive or zero).
    pub fn subtract_with_minimum(
        &mut self,
        key: &Address,
        req: &TokenAmount,
        floor: &TokenAmount,
    ) -> Result<TokenAmount, String> {
        let prev = self.get(key)?;
        let res = prev.checked_sub(req).unwrap_or_else(|| TokenAmount::new(0));
        let new_val: TokenAmount = std::cmp::max(&res, floor).clone();

        if prev > new_val {
            // Subtraction needed, set new value and return change
            self.0.set(key.hash_key(), new_val.clone())?;
            Ok(prev - new_val)
        } else {
            // New value is same as previous, no change needed
            Ok(TokenAmount::default())
        }
    }

    /// Subtracts value from a balance, and errors if full amount was not substracted.
    pub fn must_subtract(&mut self, key: &Address, req: &TokenAmount) -> Result<(), String> {
        let sub_amt = self.subtract_with_minimum(key, req, &TokenAmount::new(0))?;
        if &sub_amt != req {
            return Err(format!(
                "Couldn't subtract value from address {} (req: {}, available: {})",
                key, req, sub_amt
            ));
        }

        Ok(())
    }

    /// Removes an entry from the table, returning the prior value. The entry must have been previously initialized.
    pub fn remove(&mut self, key: &Address) -> Result<TokenAmount, String> {
        // Ensure entry exists and get previous value
        let prev = self.get(key)?;

        // Remove entry from table
        self.0.delete(&key.hash_key())?;

        Ok(prev)
    }

    /// Returns total balance held by this balance table
    pub fn total(&self) -> Result<TokenAmount, String> {
        let mut total = TokenAmount::default();

        self.0.for_each(&mut |_, v| {
            total += v;
            Ok(())
        })?;

        Ok(total)
    }
}
