// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::bitfield::Bitfield;
use super::pointer::Pointer;
use super::{Error, Hash, HashedKey, KeyValuePair, MAX_ARRAY_WIDTH};
use forest_encoding::{de::Deserializer, ser::Serializer};
use ipld_blockstore::BlockStore;
use murmur3::murmur3_x64_128::MurmurHasher;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::borrow::Borrow;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Node<K, V> {
    pub(crate) bitfield: Bitfield,
    pub(crate) pointers: Vec<Pointer<K, V>>,
}

impl<K, V> Serialize for Node<K, V>
where
    K: Serialize,
    V: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error>
    where
        S: Serializer,
    {
        (&self.bitfield, &self.pointers).serialize(serializer)
    }
}

impl<'de, K, V> Deserialize<'de> for Node<K, V>
where
    K: DeserializeOwned,
    V: DeserializeOwned,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (bitfield, pointers) = Deserialize::deserialize(deserializer)?;
        Ok(Node { bitfield, pointers })
    }
}

impl<K, V> Default for Node<K, V> {
    fn default() -> Self {
        Node {
            bitfield: Bitfield::zero(),
            pointers: Vec::new(),
        }
    }
}

impl<K, V> Node<K, V>
where
    K: Hash + Eq + std::cmp::PartialOrd + Serialize + DeserializeOwned,
    V: Serialize + DeserializeOwned,
{
    pub fn insert<S: BlockStore>(&mut self, key: K, value: V, store: &S) -> Option<V> {
        match self.modify_value(Self::hash(&key), 0, key, value, store) {
            Ok(res) => res,
            Err(_) => None,
        }
    }

    #[inline]
    pub fn get<Q: ?Sized, S: BlockStore>(&self, k: &Q, store: &S) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: Eq + Hash,
    {
        self.search(k, store).map(|kv| kv.value())
    }

    #[inline]
    pub fn remove_entry<Q: ?Sized, S>(&mut self, k: &Q, store: &S) -> Option<(K, V)>
    where
        K: Borrow<Q>,
        Q: Eq + Hash,
        S: BlockStore,
    {
        match self.rm_value(Self::hash(k), 0, k, store) {
            Ok(res) => res,
            Err(_) => None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.pointers.is_empty()
    }

    /// Search for a key.
    #[inline]
    fn search<Q: ?Sized, S: BlockStore>(&self, q: &Q, store: &S) -> Option<&KeyValuePair<K, V>>
    where
        K: Borrow<Q>,
        Q: Eq + Hash,
    {
        self.get_value(Self::hash(q), 0, q, store)
    }

    fn get_value<Q: ?Sized, S: BlockStore>(
        &self,
        hashed_key: HashedKey,
        depth: usize,
        key: &Q,
        store: &S,
    ) -> Option<&KeyValuePair<K, V>>
    where
        K: Borrow<Q>,
        Q: Eq + Hash,
    {
        assert!(depth < hashed_key.len(), "max depth reached");

        let idx = hashed_key[depth];
        if !self.bitfield.test_bit(idx) {
            return None;
        }

        let cindex = self.index_for_bit_pos(idx);
        let child = self.get_child(cindex);
        if child.is_shard() {
            match child.load_child(store) {
                Ok(chnd) => chnd.get_value(hashed_key, depth + 1, key, store),
                Err(_) => None,
            }
        } else {
            child.kvs.iter().find(|kv| key.eq(kv.key().borrow()))
        }
    }

    /// The hash function used to hash keys.
    fn hash<X: ?Sized>(key: &X) -> HashedKey
    where
        X: Hash,
    {
        let mut hasher = MurmurHasher::default();
        key.hash(&mut hasher);
        hasher.finalize().into()
    }

    /// Internal method to modify values.
    fn modify_value<S: BlockStore>(
        &mut self,
        hashed_key: HashedKey,
        depth: usize,
        key: K,
        value: V,
        store: &S,
    ) -> Result<Option<V>, Error> {
        if depth >= hashed_key.len() {
            return Err(Error::Custom("Maximum depth reached"));
        }
        let idx = hashed_key[depth];

        // No existing values at this point.
        if !self.bitfield.test_bit(idx) {
            self.insert_child(idx, key, value);
            return Ok(None);
        }

        let cindex = self.index_for_bit_pos(idx);
        let child = self.get_child_mut(cindex);

        if child.is_shard() {
            let chnd = child.load_child_mut(store)?;
            let v = chnd.modify_value(hashed_key, depth + 1, key, value, store)?;

            return Ok(v);
        }

        // Update, if the key already exists.
        if let Some(i) = child.kvs.iter().position(|p| p.key() == &key) {
            let old_value = std::mem::replace(&mut child.kvs[i].1, value);
            return Ok(Some(old_value));
        }

        // If the array is full, create a subshard and insert everything
        if child.kvs.len() > MAX_ARRAY_WIDTH {
            let mut sub = Node::default();
            sub.modify_value(hashed_key, depth + 1, key, value, store)?;
            let kvs = std::mem::replace(&mut child.kvs, Vec::new());
            for p in kvs.into_iter() {
                sub.modify_value(Self::hash(p.key()), depth + 1, p.0, p.1, store)?;
            }

            let link = store.put(&sub)?;
            self.set_child(cindex, Pointer::from_link(link, sub));
            return Ok(None);
        }

        // Otherwise insert the element into the array in order.
        let max = child.kvs.len();
        let idx = child
            .kvs
            .iter()
            .position(|c| c.key() > &key)
            .unwrap_or_else(|| max);

        let np = KeyValuePair::new(key, value);
        child.kvs.insert(idx, np);

        Ok(None)
    }

    /// Internal method to delete entries.
    pub fn rm_value<Q: ?Sized, S: BlockStore>(
        &mut self,
        hashed_key: HashedKey,
        depth: usize,
        key: &Q,
        store: &S,
    ) -> Result<Option<(K, V)>, Error>
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
    {
        if depth >= hashed_key.len() {
            return Err(Error::Custom("Maximum depth reached"));
        }
        let idx = hashed_key[depth];

        // No existing values at this point.
        if !self.bitfield.test_bit(idx) {
            return Ok(None);
        }

        let cindex = self.index_for_bit_pos(idx);
        let child = self.get_child_mut(cindex);

        if child.is_shard() {
            let chnd = child.load_child_mut(store)?;
            let v = chnd.rm_value(hashed_key, depth + 1, key, store)?;

            // CHAMP optimization, ensure trees look correct after deletion
            return child.clean().map(|_| v);
        }

        // Delete value
        for (i, p) in child.kvs.iter().enumerate() {
            if key.eq(p.key().borrow()) {
                let old = if child.kvs.len() == 1 {
                    self.rm_child(cindex, idx).kvs.into_iter().nth(0).unwrap()
                } else {
                    child.kvs.remove(i)
                };
                return Ok(Some((old.0, old.1)));
            }
        }

        Ok(None)
    }

    fn rm_child(&mut self, i: usize, idx: u8) -> Pointer<K, V> {
        self.bitfield.clear_bit(idx);
        self.pointers.remove(i)
    }

    fn insert_child(&mut self, idx: u8, key: K, value: V) {
        let i = self.index_for_bit_pos(idx);
        self.bitfield.set_bit(idx);
        self.pointers
            .insert(i as usize, Pointer::from_key_value(key, value))
    }

    fn set_child(&mut self, i: usize, pointer: Pointer<K, V>) {
        self.pointers[i] = pointer;
    }

    fn index_for_bit_pos(&self, bp: u8) -> usize {
        let mask = Bitfield::zero().set_bits_le(bp);
        assert_eq!(mask.count_ones(), bp as usize);
        mask.and(&self.bitfield).count_ones()
    }

    fn get_child_mut(&mut self, i: usize) -> &mut Pointer<K, V> {
        &mut self.pointers[i]
    }

    fn get_child<'a>(&'a self, i: usize) -> &'a Pointer<K, V> {
        &self.pointers[i]
    }
}
