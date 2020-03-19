// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::HAMT_BIT_WIDTH;
use cid::Cid;
use encoding::Cbor;
use ipld_amt::Amt;
use ipld_blockstore::BlockStore;
use ipld_hamt::{Error, Hamt};
use vm::TokenAmount;

/// Multimap stores multiple values per key in a Hamt of Amts.
/// The order of insertion of values for each key is retained.
pub struct Multimap<'a, BS>(Hamt<'a, String, BS>);
impl<'a, BS> Multimap<'a, BS>
where
    BS: BlockStore,
{
    /// Initializes a new empty multimap.
    pub fn new_empty(bs: &'a BS) -> Self {
        Self(Hamt::new_with_bit_width(bs, HAMT_BIT_WIDTH))
    }

    /// Initializes a multimap from a root Cid
    pub fn from_root(bs: &'a BS, cid: &Cid) -> Result<Self, Error> {
        Ok(Self(Hamt::load_with_bit_width(cid, bs, HAMT_BIT_WIDTH)?))
    }

    /// Retrieve root from multimap
    #[inline]
    pub fn root(&mut self) -> Result<Cid, Error> {
        self.0.flush()
    }

    /// Adds a value for a key.
    pub fn add<V>(&mut self, key: &String, _value: &V) -> Result<(), String>
    where
        V: Cbor + Clone,
    {
        let _prev = self
            .get::<V>(key)?
            .ok_or(format!("Array for key: {} does not exist", key))?;
        todo!()
    }

    /// Gets token amount for given address in multimap
    #[inline]
    pub fn get<V>(&self, key: &String) -> Result<Option<Amt<'a, V, BS>>, String>
    where
        V: Cbor + Clone,
    {
        match self.0.get(key)? {
            Some(cid) => Ok(Some(Amt::load(&cid, self.0.store())?)),
            None => Ok(None),
        }
    }

    /// Removes all values for a key.
    #[inline]
    pub fn remove_all<C: Cbor>(&mut self, key: String) -> Result<(), String> {
        // Remove entry from table
        self.0.delete(&key)?;

        Ok(())
    }

    /// Returns total balance held by this multimap
    pub fn for_each(&self) -> Result<TokenAmount, String> {
        let mut total = TokenAmount::default();

        self.0.for_each(&mut |_, v| {
            total += v;
            Ok(())
        })?;

        Ok(total)
    }
}
