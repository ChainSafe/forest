// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::bitfield::Bitfield;
use super::hash_bits::HashBits;
use super::pointer::Pointer;
use super::{Error, Hash, HashAlgorithm, KeyValuePair, MAX_ARRAY_WIDTH};
use cid::multihash::Blake2b256;
use forest_ipld::{from_ipld, Ipld};
use ipld_blockstore::BlockStore;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::borrow::Borrow;
use std::error::Error as StdError;
use std::fmt::Debug;
use std::marker::PhantomData;

/// Node in Hamt tree which contains bitfield of set indexes and pointers to nodes
#[derive(Debug, Clone)]
pub(crate) struct Node<K, H> {
    pub(crate) bitfield: Bitfield,
    pub(crate) pointers: Vec<Pointer<K, H>>,
    hash: PhantomData<H>,
}

impl<K: PartialEq, H> PartialEq for Node<K, H> {
    fn eq(&self, other: &Self) -> bool {
        (self.bitfield == other.bitfield) && (self.pointers == other.pointers)
    }
}

impl<K, H> Serialize for Node<K, H>
where
    K: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.bitfield, &self.pointers).serialize(serializer)
    }
}

impl<'de, K, H> Deserialize<'de> for Node<K, H>
where
    K: DeserializeOwned,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (bitfield, pointers) = Deserialize::deserialize(deserializer)?;
        Ok(Node {
            bitfield,
            pointers,
            hash: Default::default(),
        })
    }
}

impl<K, H> Default for Node<K, H> {
    fn default() -> Self {
        Node {
            bitfield: Bitfield::zero(),
            pointers: Vec::new(),
            hash: Default::default(),
        }
    }
}

impl<K, H> Node<K, H>
where
    K: Hash + Eq + PartialOrd + Serialize + DeserializeOwned + Clone,
    H: HashAlgorithm,
{
    pub fn set<S: BlockStore>(
        &mut self,
        key: K,
        value: Ipld,
        store: &S,
        bit_width: u32,
    ) -> Result<(), Error> {
        let hash = H::hash(&key);
        self.modify_value(&mut HashBits::new(&hash), bit_width, 0, key, value, store)
    }

    #[inline]
    pub fn get<Q: ?Sized, S: BlockStore>(
        &self,
        k: &Q,
        store: &S,
        bit_width: u32,
    ) -> Result<Option<Ipld>, Error>
    where
        K: Borrow<Q>,
        Q: Eq + Hash,
    {
        Ok(self.search(k, store, bit_width)?.map(|kv| kv.1))
    }

    #[inline]
    pub fn remove_entry<Q: ?Sized, S>(
        &mut self,
        k: &Q,
        store: &S,
        bit_width: u32,
    ) -> Result<Option<(K, Ipld)>, Error>
    where
        K: Borrow<Q>,
        Q: Eq + Hash,
        S: BlockStore,
    {
        let hash = H::hash(k);
        self.rm_value(&mut HashBits::new(&hash), bit_width, 0, k, store)
    }

    pub fn is_empty(&self) -> bool {
        self.pointers.is_empty()
    }

    pub(crate) fn for_each<V, S, F>(&self, store: &S, f: &mut F) -> Result<(), Box<dyn StdError>>
    where
        V: DeserializeOwned,
        F: FnMut(&K, V) -> Result<(), Box<dyn StdError>>,
        S: BlockStore,
    {
        for p in &self.pointers {
            match p {
                Pointer::Link(cid) => {
                    match store.get::<Node<K, H>>(cid).map_err(|e| e.to_string())? {
                        Some(node) => node.for_each(store, f)?,
                        None => return Err(format!("Node with cid {} not found", cid).into()),
                    }
                }
                Pointer::Cache(n) => n.for_each(store, f)?,
                Pointer::Values(kvs) => {
                    for kv in kvs {
                        f(kv.0.borrow(), from_ipld(&kv.1).map_err(Error::Encoding)?)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Search for a key.
    fn search<Q: ?Sized, S: BlockStore>(
        &self,
        q: &Q,
        store: &S,
        bit_width: u32,
    ) -> Result<Option<KeyValuePair<K>>, Error>
    where
        K: Borrow<Q>,
        Q: Eq + Hash,
    {
        let hash = H::hash(q);
        self.get_value(&mut HashBits::new(&hash), bit_width, 0, q, store)
    }

    fn get_value<Q: ?Sized, S: BlockStore>(
        &self,
        hashed_key: &mut HashBits,
        bit_width: u32,
        depth: usize,
        key: &Q,
        store: &S,
    ) -> Result<Option<KeyValuePair<K>>, Error>
    where
        K: Borrow<Q>,
        Q: Eq + Hash,
    {
        let idx = hashed_key.next(bit_width)?;

        if !self.bitfield.test_bit(idx) {
            return Ok(None);
        }

        let cindex = self.index_for_bit_pos(idx);
        let child = self.get_child(cindex);
        match child {
            Pointer::Link(cid) => match store.get::<Node<K, H>>(cid)? {
                Some(node) => Ok(node.get_value(hashed_key, bit_width, depth + 1, key, store)?),
                None => Err(Error::CidNotFound(cid.to_string())),
            },
            Pointer::Cache(n) => n.get_value(hashed_key, bit_width, depth + 1, key, store),
            Pointer::Values(vals) => Ok(vals.iter().find(|kv| key.eq(kv.key().borrow())).cloned()),
        }
    }

    /// Internal method to modify values.
    fn modify_value<S: BlockStore>(
        &mut self,
        hashed_key: &mut HashBits,
        bit_width: u32,
        depth: usize,
        key: K,
        value: Ipld,
        store: &S,
    ) -> Result<(), Error> {
        let idx = hashed_key.next(bit_width)?;

        // No existing values at this point.
        if !self.bitfield.test_bit(idx) {
            self.insert_child(idx, key, value);
            return Ok(());
        }

        let cindex = self.index_for_bit_pos(idx);
        let child = self.get_child_mut(cindex);

        match child {
            Pointer::Link(cid) => match store.get::<Node<K, H>>(cid)? {
                Some(mut node) => {
                    // Pull value from store and update to cached node
                    node.modify_value(hashed_key, bit_width, depth + 1, key, value, store)?;
                    *child = Pointer::Cache(Box::new(node));
                    Ok(())
                }
                None => Err(Error::CidNotFound(cid.to_string())),
            },
            Pointer::Cache(n) => {
                Ok(n.modify_value(hashed_key, bit_width, depth + 1, key, value, store)?)
            }
            Pointer::Values(vals) => {
                // Update, if the key already exists.
                if let Some(i) = vals.iter().position(|p| p.key() == &key) {
                    vals[i].1 = value;
                    return Ok(());
                }

                // If the array is full, create a subshard and insert everything
                if vals.len() >= MAX_ARRAY_WIDTH {
                    let mut sub = Node::default();
                    let consumed = hashed_key.consumed;
                    sub.modify_value(hashed_key, bit_width, depth + 1, key, value, store)?;
                    let kvs = std::mem::replace(vals, Vec::new());
                    for p in kvs.into_iter() {
                        let hash = H::hash(p.key());
                        sub.modify_value(
                            &mut HashBits::new_at_index(&hash, consumed),
                            bit_width,
                            depth + 1,
                            p.0,
                            p.1,
                            store,
                        )?;
                    }

                    *child = Pointer::Cache(Box::new(sub));
                    return Ok(());
                }

                // Otherwise insert the element into the array in order.
                let max = vals.len();
                let idx = vals
                    .iter()
                    .position(|c| c.key() > &key)
                    .unwrap_or_else(|| max);

                let np = KeyValuePair::new(key, value);
                vals.insert(idx, np);

                Ok(())
            }
        }
    }

    /// Internal method to delete entries.
    fn rm_value<Q: ?Sized, S: BlockStore>(
        &mut self,
        hashed_key: &mut HashBits,
        bit_width: u32,
        depth: usize,
        key: &Q,
        store: &S,
    ) -> Result<Option<(K, Ipld)>, Error>
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
    {
        let idx = hashed_key.next(bit_width)?;

        // No existing values at this point.
        if !self.bitfield.test_bit(idx) {
            return Ok(None);
        }

        let cindex = self.index_for_bit_pos(idx);
        let child = self.get_child_mut(cindex);

        match child {
            Pointer::Link(cid) => match store.get::<Node<K, H>>(cid)? {
                Some(mut node) => {
                    // Pull value from store and update to cached node
                    let del = node.rm_value(hashed_key, bit_width, depth + 1, key, store)?;
                    *child = Pointer::Cache(Box::new(node));

                    // Clean to retrieve canonical form
                    child.clean()?;
                    Ok(del)
                }
                None => Err(Error::CidNotFound(cid.to_string())),
            },
            Pointer::Cache(n) => {
                // Delete value and return deleted value
                let deleted = n.rm_value(hashed_key, bit_width, depth + 1, key, store)?;

                // Clean to ensure canonical form
                child.clean()?;
                Ok(deleted)
            }
            Pointer::Values(vals) => {
                // Delete value
                for (i, p) in vals.iter().enumerate() {
                    if key.eq(p.key().borrow()) {
                        let old = if vals.len() == 1 {
                            if let Pointer::Values(new_v) = self.rm_child(cindex, idx) {
                                new_v.into_iter().next().unwrap()
                            } else {
                                unreachable!()
                            }
                        } else {
                            vals.remove(i)
                        };
                        return Ok(Some((old.0, old.1)));
                    }
                }

                Ok(None)
            }
        }
    }

    pub fn flush<S: BlockStore>(&mut self, store: &S) -> Result<(), Error> {
        for pointer in &mut self.pointers {
            if let Pointer::Cache(node) = pointer {
                // Flush cached sub node to clear it's cache
                node.flush(store)?;

                // Put node in blockstore and retrieve Cid
                let cid = store.put(node, Blake2b256)?;

                // Replace cached node with Cid link
                *pointer = Pointer::Link(cid);
            }
        }

        Ok(())
    }

    fn rm_child(&mut self, i: usize, idx: u32) -> Pointer<K, H> {
        self.bitfield.clear_bit(idx);
        self.pointers.remove(i)
    }

    fn insert_child(&mut self, idx: u32, key: K, value: Ipld) {
        let i = self.index_for_bit_pos(idx);
        self.bitfield.set_bit(idx);
        self.pointers
            .insert(i as usize, Pointer::from_key_value(key, value))
    }

    fn index_for_bit_pos(&self, bp: u32) -> usize {
        let mask = Bitfield::zero().set_bits_le(bp);
        assert_eq!(mask.count_ones(), bp as usize);
        mask.and(&self.bitfield).count_ones()
    }

    fn get_child_mut(&mut self, i: usize) -> &mut Pointer<K, H> {
        &mut self.pointers[i]
    }

    fn get_child(&self, i: usize) -> &Pointer<K, H> {
        &self.pointers[i]
    }
}
