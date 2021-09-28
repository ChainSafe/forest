// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{make_empty_map, make_map_with_root, BytesKey, Map};
use cid::Cid;
use fil_types::HAMT_BIT_WIDTH;
use ipld_blockstore::BlockStore;
use ipld_hamt::Error;
use std::error::Error as StdError;

/// Set is a Hamt with empty values for the purpose of acting as a hash set.
#[derive(Debug)]
pub struct Set<'a, BS>(Map<'a, BS, ()>);

impl<'a, BS: BlockStore> PartialEq for Set<'a, BS> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<'a, BS> Set<'a, BS>
where
    BS: BlockStore,
{
    /// Initializes a new empty Set with the default bitwidth.
    pub fn new(bs: &'a BS) -> Self {
        Self(make_empty_map(bs, HAMT_BIT_WIDTH))
    }

    /// Initializes a new empty Set given a bitwidth.
    pub fn new_set_with_bitwidth(bs: &'a BS, bitwidth: u32) -> Self {
        Self(make_empty_map(bs, bitwidth))
    }

    /// Initializes a Set from a root Cid.
    pub fn from_root(bs: &'a BS, cid: &Cid) -> Result<Self, Error> {
        Ok(Self(make_map_with_root(cid, bs)?))
    }

    /// Retrieve root from the Set.
    #[inline]
    pub fn root(&mut self) -> Result<Cid, Error> {
        self.0.flush()
    }

    /// Adds key to the set.
    #[inline]
    pub fn put(&mut self, key: BytesKey) -> Result<(), Error> {
        // Set hamt node to array root
        self.0.set(key, ())?;
        Ok(())
    }

    /// Checks if key exists in the set.
    #[inline]
    pub fn has(&self, key: &[u8]) -> Result<bool, Error> {
        self.0.contains_key(key)
    }

    /// Deletes key from set.
    #[inline]
    pub fn delete(&mut self, key: &[u8]) -> Result<Option<()>, Error> {
        match self.0.delete(key)? {
            Some(_) => Ok(Some(())),
            None => Ok(None),
        }
    }

    /// Iterates through all keys in the set.
    pub fn for_each<F>(&self, mut f: F) -> Result<(), Box<dyn StdError>>
    where
        F: FnMut(&BytesKey) -> Result<(), Box<dyn StdError>>,
    {
        // Calls the for each function on the hamt with ignoring the value
        self.0.for_each(|s, _: &()| f(s))
    }

    /// Collects all keys from the set into a vector.
    pub fn collect_keys(&self) -> Result<Vec<BytesKey>, Error> {
        let mut ret_keys = Vec::new();

        self.for_each(|k| {
            ret_keys.push(k.clone());
            Ok(())
        })?;

        Ok(ret_keys)
    }
}
