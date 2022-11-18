use crate::{make_empty_map, make_map_with_root_and_bitwidth, Keyer, Map};
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_hamt::{BytesKey, Error};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde::__private::PhantomData;
use std::collections::btree_map::Entry::{Occupied, Vacant};
use std::collections::BTreeMap;

// MapMap stores multiple values per key in a Hamt of Hamts
// Every element stored has a primary and secondary key
pub struct MapMap<'a, BS, V, K1, K2> {
    outer: Map<'a, BS, Cid>,
    inner_bitwidth: u32,
    // cache all inner maps loaded since last load/flush
    // get/put/remove operations load the inner map into the cache first and modify in memory
    // flush writes all inner maps in the cache to the outer map before flushing the outer map
    cache: BTreeMap<Vec<u8>, Map<'a, BS, V>>,
    key_types: PhantomData<(K1, K2)>,
}
impl<'a, BS, V, K1, K2> MapMap<'a, BS, V, K1, K2>
where
    BS: Blockstore,
    V: Serialize + DeserializeOwned + Clone + std::cmp::PartialEq,
    K1: Keyer + std::fmt::Debug + std::fmt::Display,
    K2: Keyer + std::fmt::Debug + std::fmt::Display,
{
    pub fn new(bs: &'a BS, outer_bitwidth: u32, inner_bitwidth: u32) -> Self {
        MapMap {
            outer: make_empty_map(bs, outer_bitwidth),
            inner_bitwidth,
            cache: BTreeMap::<Vec<u8>, Map<BS, V>>::new(),
            key_types: PhantomData,
        }
    }

    pub fn from_root(
        bs: &'a BS,
        cid: &Cid,
        outer_bitwidth: u32,
        inner_bitwidth: u32,
    ) -> Result<Self, Error> {
        Ok(MapMap {
            outer: make_map_with_root_and_bitwidth(cid, bs, outer_bitwidth)?,
            inner_bitwidth,
            cache: BTreeMap::<Vec<u8>, Map<BS, V>>::new(),
            key_types: PhantomData,
        })
    }

    pub fn flush(&mut self) -> Result<Cid, Error> {
        for (k, in_map) in self.cache.iter_mut() {
            if in_map.is_empty() {
                self.outer.delete(&BytesKey(k.to_vec()))?;
            } else {
                let new_in_root = in_map.flush()?;
                self.outer.set(BytesKey(k.to_vec()), new_in_root)?;
            }
        }
        self.outer.flush()
    }

    // load inner map while memoizing
    // 1. ensure inner map is loaded into cache
    // 2. return (inner map is empty, inner map)
    fn load_inner_map(&mut self, k: K1) -> Result<(bool, &mut Map<'a, BS, V>), Error> {
        let in_map_thunk = || -> Result<(bool, Map<BS, V>), Error> {
            // lazy to avoid ipld operations in case of cache hit
            match self.outer.get(&k.key())? {
                // flush semantics guarantee all written inner maps are non empty
                Some(root) => Ok((
                    false,
                    make_map_with_root_and_bitwidth::<BS, V>(
                        root,
                        *self.outer.store(),
                        self.inner_bitwidth,
                    )?,
                )),
                None => Ok((
                    true,
                    make_empty_map(*self.outer.store(), self.inner_bitwidth),
                )),
            }
        };
        let raw_k = k.key().0;
        match self.cache.entry(raw_k) {
            Occupied(entry) => {
                let in_map = entry.into_mut();
                // cached map could be empty
                Ok((in_map.is_empty(), in_map))
            }
            Vacant(entry) => {
                let (empty, in_map) = in_map_thunk()?;
                Ok((empty, entry.insert(in_map)))
            }
        }
    }

    pub fn get(&mut self, outside_k: K1, inside_k: K2) -> Result<Option<&V>, Error> {
        let (is_empty, in_map) = self.load_inner_map(outside_k)?;
        if is_empty {
            return Ok(None);
        }
        in_map.get(&inside_k.key())
    }

    // Runs a function over all values for one outer key.
    pub fn for_each<F>(&mut self, outside_k: K1, f: F) -> Result<(), Error>
    where
        F: FnMut(&BytesKey, &V) -> anyhow::Result<()>,
    {
        let (is_empty, in_map) = self.load_inner_map(outside_k)?;
        if is_empty {
            return Ok(());
        }
        in_map.for_each(f)
    }

    // Puts a key value pair in the MapMap, overwriting any existing value.
    // Returns the previous value, if any.
    pub fn put(&mut self, outside_k: K1, inside_k: K2, value: V) -> Result<Option<V>, Error> {
        let in_map = self.load_inner_map(outside_k)?.1;
        // defer flushing cached inner map until flush call
        in_map.set(inside_k.key(), value)
    }

    // Puts a key value pair in the MapMap if it is not already set.  Returns true
    // if key is newly set, false if it was already set.
    pub fn put_if_absent(&mut self, outside_k: K1, inside_k: K2, value: V) -> Result<bool, Error> {
        let in_map = self.load_inner_map(outside_k)?.1;

        // defer flushing cached inner map until flush call
        in_map.set_if_absent(inside_k.key(), value)
    }

    // Puts many values in the MapMap under a single outside key.
    // Overwrites any existing values.
    pub fn put_many<I>(&mut self, outside_k: K1, values: I) -> Result<(), Error>
    where
        I: Iterator<Item = (K2, V)>,
    {
        let in_map = self.load_inner_map(outside_k)?.1;
        for (k, v) in values {
            in_map.set(k.key(), v)?;
        }
        // defer flushing cached inner map until flush call
        Ok(())
    }

    /// Removes a key from the MapMap, returning the value at the key if the key
    /// was previously set.
    pub fn remove(&mut self, outside_k: K1, inside_k: K2) -> Result<Option<V>, Error> {
        let (is_empty, in_map) = self.load_inner_map(outside_k)?;
        if is_empty {
            return Ok(None);
        }
        in_map
            .delete(&inside_k.key())
            .map(|o: Option<(BytesKey, V)>| -> Option<V> { o.map(|p: (BytesKey, V)| -> V { p.1 }) })
    }
}
