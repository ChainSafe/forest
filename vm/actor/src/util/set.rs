// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{EmptyType, EMPTY_VALUE, HAMT_BIT_WIDTH};
use cid::Cid;
use ipld_blockstore::BlockStore;
use ipld_hamt::{Error, Hamt};

/// Set is a Hamt with empty values for the purpose of acting as a hash set.
#[derive(Debug)]
pub struct Set<'a, BS>(Hamt<'a, String, BS>);

impl<'a, BS: BlockStore> PartialEq for Set<'a, BS> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<'a, BS> Set<'a, BS>
where
    BS: BlockStore,
{
    /// Initializes a new empty Set.
    pub fn new(bs: &'a BS) -> Self {
        Self(Hamt::new_with_bit_width(bs, HAMT_BIT_WIDTH))
    }

    /// Initializes a Set from a root Cid.
    pub fn from_root(bs: &'a BS, cid: &Cid) -> Result<Self, Error> {
        Ok(Self(Hamt::load_with_bit_width(cid, bs, HAMT_BIT_WIDTH)?))
    }

    /// Retrieve root from the Set.
    #[inline]
    pub fn root(&mut self) -> Result<Cid, Error> {
        self.0.flush()
    }

    /// Adds key to the set.
    #[inline]
    pub fn put(&mut self, key: String) -> Result<(), String> {
        // Set hamt node to array root
        Ok(self.0.set(key, EMPTY_VALUE)?)
    }

    /// Checks if key exists in the set.
    #[inline]
    pub fn has(&self, key: &str) -> Result<bool, String> {
        Ok(self.0.get::<_, EmptyType>(key)?.is_some())
    }

    /// Deletes key from set.
    #[inline]
    pub fn delete(&mut self, key: &str) -> Result<(), String> {
        self.0.delete(key)?;

        Ok(())
    }

    /// Iterates through all keys in the set.
    pub fn for_each<F>(&self, mut f: F) -> Result<(), String>
    where
        F: FnMut(&String) -> Result<(), String>,
    {
        // Calls the for each function on the hamt with ignoring the value
        self.0.for_each(&mut |s, _: EmptyType| f(s))
    }

    /// Collects all keys from the set into a vector.
    pub fn collect_keys(&self) -> Result<Vec<String>, String> {
        let mut ret_keys = Vec::new();

        self.for_each(|k| {
            ret_keys.push(k.clone());
            Ok(())
        })?;

        Ok(ret_keys)
    }
}
