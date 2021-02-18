// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{make_empty_map, make_map_with_root_and_bitwidth, BytesKey, Map};
use cid::Cid;
use ipld_amt::Amt;
use ipld_blockstore::BlockStore;
use ipld_hamt::Error;
use serde::{de::DeserializeOwned, Serialize};
use std::error::Error as StdError;

/// Multimap stores multiple values per key in a Hamt of Amts.
/// The order of insertion of values for each key is retained.
pub struct Multimap<'a, BS>(Map<'a, BS, Cid>, usize);
impl<'a, BS> Multimap<'a, BS>
where
    BS: BlockStore,
{
    /// Initializes a new empty multimap.
    /// The outer_bitwidth is the width of the HAMT and the
    /// inner_bitwidth is the width of the AMTs inside of it.
    pub fn new(bs: &'a BS, outer_bitwidth: u32, inner_bitwidth: usize) -> Self {
        Self(make_empty_map(bs, outer_bitwidth), inner_bitwidth)
    }

    /// Initializes a multimap from a root Cid
    pub fn from_root(
        bs: &'a BS,
        cid: &Cid,
        outer_bitwidth: u32,
        inner_bitwidth: usize,
    ) -> Result<Self, Error> {
        Ok(Self(
            make_map_with_root_and_bitwidth(cid, bs, outer_bitwidth)?,
            inner_bitwidth,
        ))
    }

    /// Retrieve root from the multimap.
    #[inline]
    pub fn root(&mut self) -> Result<Cid, Error> {
        self.0.flush()
    }

    /// Adds a value for a key.
    pub fn add<V>(&mut self, key: BytesKey, value: V) -> Result<(), Box<dyn StdError>>
    where
        V: Serialize + DeserializeOwned,
    {
        // Get construct amt from retrieved cid or create new
        let mut arr = self
            .get::<V>(&key)?
            .unwrap_or_else(|| Amt::new_with_bit_width(self.0.store(), self.1));

        // Set value at next index
        arr.set(arr.count(), value)?;

        // flush to get new array root to put in hamt
        let new_root = arr.flush()?;

        // Set hamt node to array root
        self.0.set(key, new_root)?;
        Ok(())
    }

    /// Gets the Array of value type `V` using the multimap store.
    #[inline]
    pub fn get<V>(&self, key: &[u8]) -> Result<Option<Amt<'a, V, BS>>, Box<dyn StdError>>
    where
        V: DeserializeOwned + Serialize,
    {
        match self.0.get(key)? {
            Some(cid) => Ok(Some(Amt::load(&cid, self.0.store())?)),
            None => Ok(None),
        }
    }

    /// Removes all values for a key.
    #[inline]
    pub fn remove_all(&mut self, key: &[u8]) -> Result<(), Box<dyn StdError>> {
        // Remove entry from table
        self.0
            .delete(key)?
            .ok_or("failed to delete from multimap")?;

        Ok(())
    }

    /// Iterates through all values in the array at a given key.
    pub fn for_each<F, V>(&self, key: &[u8], f: F) -> Result<(), Box<dyn StdError>>
    where
        V: Serialize + DeserializeOwned,
        F: FnMut(usize, &V) -> Result<(), Box<dyn StdError>>,
    {
        if let Some(amt) = self.get::<V>(key)? {
            amt.for_each(f)?;
        }

        Ok(())
    }

    /// Iterates through all arrays in the multimap
    pub fn for_all<F, V>(&self, mut f: F) -> Result<(), Box<dyn StdError>>
    where
        V: Serialize + DeserializeOwned,
        F: FnMut(&BytesKey, &Amt<V, BS>) -> Result<(), Box<dyn StdError>>,
    {
        self.0.for_each::<_>(|key, arr_root| {
            let arr = Amt::load(&arr_root, self.0.store())?;
            f(key, &arr)
        })?;

        Ok(())
    }
}
