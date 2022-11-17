// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_hamt::Error;
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::{make_empty_map, make_map_with_root_and_bitwidth, Array, BytesKey, Map};

/// Multimap stores multiple values per key in a Hamt of Amts.
/// The order of insertion of values for each key is retained.
pub struct Multimap<'a, BS>(Map<'a, BS, Cid>, u32);
impl<'a, BS> Multimap<'a, BS>
where
    BS: Blockstore,
{
    /// Initializes a new empty multimap.
    /// The outer_bitwidth is the width of the HAMT and the
    /// inner_bitwidth is the width of the AMTs inside of it.
    pub fn new(bs: &'a BS, outer_bitwidth: u32, inner_bitwidth: u32) -> Self {
        Self(make_empty_map(bs, outer_bitwidth), inner_bitwidth)
    }

    /// Initializes a multimap from a root Cid
    pub fn from_root(
        bs: &'a BS,
        cid: &Cid,
        outer_bitwidth: u32,
        inner_bitwidth: u32,
    ) -> Result<Self, Error> {
        Ok(Self(make_map_with_root_and_bitwidth(cid, bs, outer_bitwidth)?, inner_bitwidth))
    }

    /// Retrieve root from the multimap.
    #[inline]
    pub fn root(&mut self) -> Result<Cid, Error> {
        self.0.flush()
    }

    /// Adds a value for a key.
    pub fn add<V>(&mut self, key: BytesKey, value: V) -> Result<(), Error>
    where
        V: Serialize + DeserializeOwned,
    {
        // Get construct amt from retrieved cid or create new
        let mut arr = self
            .get::<V>(&key)?
            .unwrap_or_else(|| Array::new_with_bit_width(self.0.store(), self.1));

        // Set value at next index
        arr.set(arr.count(), value).map_err(|e| anyhow::anyhow!(e))?;

        // flush to get new array root to put in hamt
        let new_root = arr.flush().map_err(|e| anyhow::anyhow!(e))?;

        // Set hamt node to array root
        self.0.set(key, new_root)?;
        Ok(())
    }

    /// Gets the Array of value type `V` using the multimap store.
    #[inline]
    pub fn get<V>(&self, key: &[u8]) -> Result<Option<Array<'a, V, BS>>, Error>
    where
        V: DeserializeOwned + Serialize,
    {
        match self.0.get(key)? {
            Some(cid) => {
                Ok(Some(Array::load(cid, *self.0.store()).map_err(|e| anyhow::anyhow!(e))?))
            }
            None => Ok(None),
        }
    }

    /// Removes all values for a key.
    #[inline]
    pub fn remove_all(&mut self, key: &[u8]) -> Result<(), Error> {
        // Remove entry from table
        self.0.delete(key)?.ok_or("failed to delete from multimap")?;

        Ok(())
    }

    /// Iterates through all values in the array at a given key.
    pub fn for_each<F, V>(&self, key: &[u8], f: F) -> Result<(), Error>
    where
        V: Serialize + DeserializeOwned,
        F: FnMut(u64, &V) -> anyhow::Result<()>,
    {
        if let Some(amt) = self.get::<V>(key)? {
            amt.for_each(f).map_err(|e| anyhow::anyhow!(e))?;
        }

        Ok(())
    }

    /// Iterates through all arrays in the multimap
    pub fn for_all<F, V>(&self, mut f: F) -> Result<(), Error>
    where
        V: Serialize + DeserializeOwned,
        F: FnMut(&BytesKey, &Array<V, BS>) -> anyhow::Result<()>,
    {
        self.0.for_each::<_>(|key, arr_root| {
            let arr = Array::load(arr_root, *self.0.store())?;
            f(key, &arr)
        })?;

        Ok(())
    }
}
