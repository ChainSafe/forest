// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{make_map, make_map_with_root, BytesKey, Map};
use cid::Cid;
use ipld_amt::Amt;
use ipld_blockstore::BlockStore;
use ipld_hamt::Error;
use serde::{de::DeserializeOwned, Serialize};
use std::error::Error as StdError;

/// Multimap stores multiple values per key in a Hamt of Amts.
/// The order of insertion of values for each key is retained.
pub struct Multimap<'a, BS>(Map<'a, BS, Cid>);
impl<'a, BS> Multimap<'a, BS>
where
    BS: BlockStore,
{
    /// Initializes a new empty multimap.
    pub fn new(bs: &'a BS) -> Self {
        Self(make_map(bs))
    }

    /// Initializes a multimap from a root Cid
    pub fn from_root(bs: &'a BS, cid: &Cid) -> Result<Self, Error> {
        Ok(Self(make_map_with_root(cid, bs)?))
    }

    /// Retrieve root from the multimap.
    #[inline]
    pub fn root(&mut self) -> Result<Cid, Error> {
        self.0.flush()
    }

    /// Adds a value for a key.
    pub fn add<V>(&mut self, key: BytesKey, value: V) -> Result<(), String>
    where
        V: Serialize + DeserializeOwned + Clone,
    {
        // Get construct amt from retrieved cid or create new
        let mut arr = self
            .get::<V>(&key)?
            .unwrap_or_else(|| Amt::new(self.0.store()));

        // Set value at next index
        arr.set(arr.count(), value)?;

        // flush to get new array root to put in hamt
        let new_root = arr.flush()?;

        // Set hamt node to array root
        Ok(self.0.set(key, new_root)?)
    }

    /// Gets the Array of value type `V` using the multimap store.
    #[inline]
    pub fn get<V>(&self, key: &[u8]) -> Result<Option<Amt<'a, V, BS>>, String>
    where
        V: DeserializeOwned + Serialize + Clone,
    {
        match self.0.get(key)? {
            Some(cid) => Ok(Some(Amt::load(&cid, self.0.store())?)),
            None => Ok(None),
        }
    }

    /// Removes all values for a key.
    #[inline]
    pub fn remove_all(&mut self, key: &[u8]) -> Result<(), String> {
        // Remove entry from table
        self.0.delete(key)?;

        Ok(())
    }

    /// Iterates through all values in the array at a given key.
    pub fn for_each<F, V>(&self, key: &[u8], f: F) -> Result<(), Box<dyn StdError>>
    where
        V: Serialize + DeserializeOwned + Clone,
        F: FnMut(u64, &V) -> Result<(), Box<dyn StdError>>,
    {
        if let Some(amt) = self.get::<V>(key)? {
            amt.for_each(f)?;
        }

        Ok(())
    }

    /// Iterates through all arrays in the multimap
    pub fn for_all<F, V>(&self, mut f: F) -> Result<(), Box<dyn StdError>>
    where
        V: Serialize + DeserializeOwned + Clone,
        F: FnMut(&BytesKey, &Amt<V, BS>) -> Result<(), Box<dyn StdError>>,
    {
        self.0.for_each::<_>(|key, arr_root| {
            let arr = Amt::load(&arr_root, self.0.store())?;
            f(key, &arr)
        })?;

        Ok(())
    }
}
