// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::bitfield::Bitfield;
use super::hash_bits::HashBits;
use super::pointer::Pointer;
use super::{Error, Hash, HashedKey, KeyValuePair, MAX_ARRAY_WIDTH};
use cid::multihash::Blake2b256;
use forest_ipld::{from_ipld, Ipld};
use ipld_blockstore::BlockStore;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::borrow::Borrow;
use std::fmt::Debug;

#[cfg(not(feature = "identity-hash"))]
use murmur3::murmur3_x64_128::MurmurHasher;

#[cfg(feature = "identity-hash")]
use std::hash::Hasher;

#[cfg(feature = "identity-hash")]
#[derive(Default)]
struct IdentityHasher {
    bz: HashedKey,
}
#[cfg(feature = "identity-hash")]
impl Hasher for IdentityHasher {
    fn finish(&self) -> u64 {
        // u64 hash not used in hamt
        0
    }

    fn write(&mut self, bytes: &[u8]) {
        for (i, byte) in bytes.iter().take(16).enumerate() {
            self.bz[i] = *byte;
        }
    }
}

/// Node in Hamt tree which contains bitfield of set indexes and pointers to nodes
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Node<K> {
    pub(crate) bitfield: Bitfield,
    pub(crate) pointers: Vec<Pointer<K>>,
}

impl<K> Serialize for Node<K>
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

impl<'de, K> Deserialize<'de> for Node<K>
where
    K: DeserializeOwned,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (bitfield, pointers) = Deserialize::deserialize(deserializer)?;
        Ok(Node { bitfield, pointers })
    }
}

impl<K> Default for Node<K> {
    fn default() -> Self {
        Node {
            bitfield: Bitfield::zero(),
            pointers: Vec::new(),
        }
    }
}

impl<K> Node<K>
where
    K: Hash + Eq + PartialOrd + Serialize + DeserializeOwned + Clone,
{
    pub fn set<S: BlockStore>(
        &mut self,
        key: K,
        value: Ipld,
        store: &S,
        bit_width: u8,
    ) -> Result<(), Error> {
        let hash = Self::hash(&key);
        self.modify_value(&mut HashBits::new(&hash), bit_width, 0, key, value, store)
    }

    #[inline]
    pub fn get<Q: ?Sized, S: BlockStore>(
        &self,
        k: &Q,
        store: &S,
        bit_width: u8,
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
        bit_width: u8,
    ) -> Result<Option<(K, Ipld)>, Error>
    where
        K: Borrow<Q>,
        Q: Eq + Hash,
        S: BlockStore,
    {
        let hash = Self::hash(k);
        self.rm_value(&mut HashBits::new(&hash), bit_width, 0, k, store)
    }

    pub fn is_empty(&self) -> bool {
        self.pointers.is_empty()
    }

    pub(crate) fn for_each<V, S, F>(&self, store: &S, f: &mut F) -> Result<(), String>
    where
        V: DeserializeOwned,
        F: FnMut(&K, V) -> Result<(), String>,
        S: BlockStore,
    {
        for p in &self.pointers {
            match p {
                Pointer::Link(cid) => match store.get::<Node<K>>(cid)? {
                    Some(node) => node.for_each(store, f)?,
                    None => return Err(format!("Node with cid {} not found", cid)),
                },
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
        bit_width: u8,
    ) -> Result<Option<KeyValuePair<K>>, Error>
    where
        K: Borrow<Q>,
        Q: Eq + Hash,
    {
        let hash = Self::hash(q);
        self.get_value(&mut HashBits::new(&hash), bit_width, 0, q, store)
    }

    fn get_value<Q: ?Sized, S: BlockStore>(
        &self,
        hashed_key: &mut HashBits,
        bit_width: u8,
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
            Pointer::Link(cid) => match store.get::<Node<K>>(cid)? {
                Some(node) => Ok(node.get_value(hashed_key, bit_width, depth + 1, key, store)?),
                None => Err(Error::Custom("Node not found")),
            },
            Pointer::Cache(n) => n.get_value(hashed_key, bit_width, depth + 1, key, store),
            Pointer::Values(vals) => Ok(vals.iter().find(|kv| key.eq(kv.key().borrow())).cloned()),
        }
    }

    /// The hash function used to hash keys.
    #[cfg(not(feature = "identity-hash"))]
    fn hash<X: ?Sized>(key: &X) -> HashedKey
    where
        X: Hash,
    {
        let mut hasher = MurmurHasher::default();
        key.hash(&mut hasher);
        hasher.finalize().into()
    }

    /// Replace hash with an identity hash for testing canonical structure.
    #[cfg(feature = "identity-hash")]
    fn hash<X: ?Sized>(key: &X) -> HashedKey
    where
        X: Hash,
    {
        let mut ident_hasher = IdentityHasher::default();
        key.hash(&mut ident_hasher);
        ident_hasher.bz
    }

    /// Internal method to modify values.
    fn modify_value<S: BlockStore>(
        &mut self,
        hashed_key: &mut HashBits,
        bit_width: u8,
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
            Pointer::Link(cid) => match store.get::<Node<K>>(cid)? {
                Some(mut node) => {
                    // Pull value from store and update to cached node
                    node.modify_value(hashed_key, bit_width, depth + 1, key, value, store)?;
                    *child = Pointer::Cache(Box::new(node));
                    Ok(())
                }
                None => Err(Error::Custom("Node not found")),
            },
            Pointer::Cache(n) => {
                Ok(n.modify_value(hashed_key, bit_width, depth + 1, key, value, store)?)
            }
            Pointer::Values(vals) => {
                // Update, if the key already exists.
                if let Some(i) = vals.iter().position(|p| p.key() == &key) {
                    std::mem::replace(&mut vals[i].1, value);
                    return Ok(());
                }

                // If the array is full, create a subshard and insert everything
                if vals.len() >= MAX_ARRAY_WIDTH {
                    let mut sub = Node::default();
                    let consumed = hashed_key.consumed;
                    sub.modify_value(hashed_key, bit_width, depth + 1, key, value, store)?;
                    let kvs = std::mem::replace(vals, Vec::new());
                    for p in kvs.into_iter() {
                        let hash = Self::hash(p.key());
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
        bit_width: u8,
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
            Pointer::Link(cid) => match store.get::<Node<K>>(cid)? {
                Some(mut node) => {
                    // Pull value from store and update to cached node
                    let del = node.rm_value(hashed_key, bit_width, depth + 1, key, store)?;
                    *child = Pointer::Cache(Box::new(node));

                    // Clean to retrieve canonical form
                    child.clean()?;
                    Ok(del)
                }
                None => Err(Error::Custom("Node not found")),
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
                                return Err(Error::Custom("Should not reach this"));
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

    fn rm_child(&mut self, i: usize, idx: u8) -> Pointer<K> {
        self.bitfield.clear_bit(idx);
        self.pointers.remove(i)
    }

    fn insert_child(&mut self, idx: u8, key: K, value: Ipld) {
        let i = self.index_for_bit_pos(idx);
        self.bitfield.set_bit(idx);
        self.pointers
            .insert(i as usize, Pointer::from_key_value(key, value))
    }

    fn index_for_bit_pos(&self, bp: u8) -> usize {
        let mask = Bitfield::zero().set_bits_le(bp);
        assert_eq!(mask.count_ones(), bp as usize);
        mask.and(&self.bitfield).count_ones()
    }

    fn get_child_mut(&mut self, i: usize) -> &mut Pointer<K> {
        &mut self.pointers[i]
    }

    fn get_child(&self, i: usize) -> &Pointer<K> {
        &self.pointers[i]
    }
}
