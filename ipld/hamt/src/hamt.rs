// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::node::Node;
use crate::{Error, Hash, DEFAULT_BIT_WIDTH};
use cid::{multihash::Blake2b256, Cid};
use ipld_blockstore::BlockStore;
use serde::{de::DeserializeOwned, Serialize, Serializer};
use std::borrow::Borrow;

/// Implementation of the HAMT data structure for IPLD.
///
/// # Examples
///
/// ```
/// use ipld_hamt::Hamt;
///
/// let store = db::MemoryDB::default();
///
/// let mut map: Hamt<usize, String, _> = Hamt::new(&store);
/// map.set(1, "a".to_string()).unwrap();
/// assert_eq!(map.get(&1).unwrap(), Some("a".to_string()));
/// assert_eq!(map.delete(&1).unwrap(), Some("a".to_string()));
/// assert_eq!(map.get(&1).unwrap(), None);
/// let cid = map.flush().unwrap();
/// ```
#[derive(Debug)]
pub struct Hamt<'a, K, V, S> {
    root: Node<K, V>,
    store: &'a S,

    bit_width: u8,
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
    K: Hash + Eq + PartialOrd + Serialize + DeserializeOwned + Clone,
    V: Serialize + DeserializeOwned + Clone,
    S: BlockStore,
{
    pub fn new(store: &'a S) -> Self {
        Self::new_with_bit_width(store, DEFAULT_BIT_WIDTH)
    }

    /// Construct hamt with a bit width
    pub fn new_with_bit_width(store: &'a S, bit_width: u8) -> Self {
        Self {
            root: Node::default(),
            store,
            bit_width,
        }
    }

    /// Lazily instantiate a hamt from this root Cid.
    pub fn load(cid: &Cid, store: &'a S) -> Result<Self, Error> {
        Self::load_with_bit_width(cid, store, DEFAULT_BIT_WIDTH)
    }

    /// Lazily instantiate a hamt from this root Cid with a specified bit width.
    pub fn load_with_bit_width(cid: &Cid, store: &'a S, bit_width: u8) -> Result<Self, Error> {
        match store.get(cid)? {
            Some(root) => Ok(Self {
                root,
                store,
                bit_width,
            }),
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
    /// # Examples
    ///
    /// ```
    /// use ipld_hamt::Hamt;
    ///
    /// let store = db::MemoryDB::default();
    ///
    /// let mut map: Hamt<usize, String, _> = Hamt::new(&store);
    /// assert_eq!(map.set(37, "a".into()).unwrap(), None);
    /// assert_eq!(map.is_empty(), false);
    ///
    /// map.set(37, "b".into()).unwrap();
    /// assert_eq!(map.set(37, "c".into()).unwrap(), Some("b".into()));
    /// ```
    pub fn set(&mut self, key: K, value: V) -> Result<Option<V>, Error> {
        self.root.set(key, value, self.store, self.bit_width)
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
    /// map.set(1, "a".to_string()).unwrap();
    /// assert_eq!(map.get(&1).unwrap(), Some("a".to_string()));
    /// assert_eq!(map.get(&2).unwrap(), None);
    /// ```
    #[inline]
    pub fn get<Q: ?Sized>(&self, k: &Q) -> Result<Option<V>, Error>
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
    {
        self.root.get(k, self.store, self.bit_width)
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
    /// map.set(1, "a".to_string()).unwrap();
    /// assert_eq!(map.delete(&1).unwrap(), Some("a".to_string()));
    /// assert_eq!(map.delete(&1).unwrap(), None);
    /// ```
    pub fn delete<Q: ?Sized>(&mut self, k: &Q) -> Result<Option<V>, Error>
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
    {
        Ok(self
            .root
            .remove_entry(k, self.store, self.bit_width)?
            .map(|kv| kv.1))
    }

    /// Flush root and return Cid for hamt
    pub fn flush(&mut self) -> Result<Cid, Error> {
        self.root.flush(self.store)?;
        Ok(self.store.put(&self.root, Blake2b256)?)
    }

    /// Returns true if the HAMT has no entries
    pub fn is_empty(&self) -> bool {
        self.root.is_empty()
    }
}
