// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::node::Node;
use crate::{Error, Hash};
use cid::Cid;
use forest_encoding::{de::DeserializeOwned, ser::Serializer};
use ipld_blockstore::BlockStore;
use serde::Serialize;
use std::borrow::Borrow;
use std::ops::Index;

/// Implementation of the HAMT data structure for IPLD.
#[derive(Debug)]
pub struct Hamt<'a, K, V, S> {
    root: Node<K, V>,
    store: &'a S,
}

impl<K, V, BS> Serialize for Hamt<'_, K, V, BS>
where
    K: Serialize,
    V: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.root.serialize(serializer)
    }
}

impl<'a, K: PartialEq, V: PartialEq, S: BlockStore> PartialEq for Hamt<'a, K, V, S> {
    fn eq(&self, other: &Self) -> bool {
        self.root == other.root
    }
}

impl<'a, K, V, S> Hamt<'a, K, V, S>
where
    K: Hash + Eq + std::cmp::PartialOrd + Serialize + DeserializeOwned,
    V: Serialize + DeserializeOwned,
    S: BlockStore,
{
    pub fn new(store: &'a S) -> Self {
        Hamt {
            root: Node::default(),
            store,
        }
    }

    /// Lazily instantiate a hamt from this root link.
    pub fn from_link(cid: &Cid, store: &'a S) -> Result<Self, Error> {
        match store.get(cid)? {
            Some(root) => Ok(Hamt { root, store }),
            None => Err(Error::Custom("No node found")),
        }
    }

    /// Inserts a key-value pair into the HAMT.
    ///
    /// If the HAMT did not have this key present, `None` is returned.
    ///
    /// If the HAMT did have this key present, the value is updated, and the old
    /// value is returned. The key is not updated, though;
    ///
    ///
    /// # Examples
    ///
    /// ```
    /// use ipld_hamt::Hamt;
    ///
    /// let store = db::MemoryDB::default();
    ///
    /// let mut map: Hamt<usize, String, _> = Hamt::new(&store);
    /// assert_eq!(map.insert(37, "a".into()), None);
    /// assert_eq!(map.is_empty(), false);
    ///
    /// map.insert(37, "b".into());
    /// assert_eq!(map.insert(37, "c".into()), Some("b".into()));
    /// assert_eq!(map[&37], "c".to_string());
    /// ```
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        self.root.insert(key, value, self.store)
    }

    /// Returns a reference to the value corresponding to the key.
    ///
    /// The key may be any borrowed form of the map's key type, but
    /// `Hash` and `Eq` on the borrowed form *must* match those for
    /// the key type.
    ///
    /// # Examples
    ///
    /// ```
    /// use ipld_hamt::Hamt;
    ///
    /// let store = db::MemoryDB::default();
    ///
    /// let mut map: Hamt<usize, String, _> = Hamt::new(&store);
    /// map.insert(1, "a".to_string());
    /// assert_eq!(map.get(&1), Some(&"a".to_string()));
    /// assert_eq!(map.get(&2), None);
    /// ```
    #[inline]
    pub fn get<Q: ?Sized>(&self, k: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
    {
        self.root.get(k, self.store)
    }

    /// Removes a key from the HAMT, returning the value at the key if the key
    /// was previously in the HAMT.
    ///
    /// The key may be any borrowed form of the HAMT's key type, but
    /// `Hash` and `Eq` on the borrowed form *must* match those for
    /// the key type.
    ///
    /// # Examples
    ///
    /// ```
    /// use ipld_hamt::Hamt;
    ///
    /// let store = db::MemoryDB::default();
    ///
    /// let mut map: Hamt<usize, String, _> = Hamt::new(&store);
    /// map.insert(1, "a".to_string());
    /// assert_eq!(map.remove(&1), Some("a".to_string()));
    /// assert_eq!(map.remove(&1), None);
    /// ```
    pub fn remove<Q: ?Sized>(&mut self, k: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
    {
        self.root.remove_entry(k, self.store).map(|kv| kv.1)
    }

    /// Returns true if the HAMT has no entries
    pub fn is_empty(&self) -> bool {
        self.root.is_empty()
    }
}

impl<'a, K, Q: ?Sized, V, S> Index<&Q> for Hamt<'a, K, V, S>
where
    K: Eq + Hash + Borrow<Q> + PartialOrd + Serialize + DeserializeOwned,
    Q: Eq + Hash,
    V: Serialize + DeserializeOwned,
    S: BlockStore,
{
    type Output = V;

    /// Returns a reference to the value corresponding to the supplied key.
    ///
    /// # Panics
    ///
    /// Panics if the key is not present in the `Hamt`.
    #[inline]
    fn index(&self, key: &Q) -> &V {
        self.get(key).expect("no entry found for key")
    }
}
