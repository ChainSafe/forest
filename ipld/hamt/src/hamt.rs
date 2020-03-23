// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::node::Node;
use crate::{Error, Hash, DEFAULT_BIT_WIDTH};
use cid::{multihash::Blake2b256, Cid};
use forest_ipld::{from_ipld, to_ipld, Ipld};
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
/// let mut map: Hamt<usize, _> = Hamt::new(&store);
/// map.set(1, "a".to_string()).unwrap();
/// assert_eq!(map.get(&1).unwrap(), Some("a".to_string()));
/// assert_eq!(map.delete(&1).unwrap(), true);
/// assert_eq!(map.get::<_, String>(&1).unwrap(), None);
/// let cid = map.flush().unwrap();
/// ```
#[derive(Debug)]
pub struct Hamt<'a, K, BS> {
    root: Node<K>,
    store: &'a BS,

    bit_width: u8,
}

impl<K, BS> Serialize for Hamt<'_, K, BS>
where
    K: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.root.serialize(serializer)
    }
}

impl<'a, K: PartialEq, S: BlockStore> PartialEq for Hamt<'a, K, S> {
    fn eq(&self, other: &Self) -> bool {
        self.root == other.root
    }
}

impl<'a, K, BS> Hamt<'a, K, BS>
where
    K: Hash + Eq + PartialOrd + Serialize + DeserializeOwned + Clone,
    BS: BlockStore,
{
    pub fn new(store: &'a BS) -> Self {
        Self::new_with_bit_width(store, DEFAULT_BIT_WIDTH)
    }

    /// Construct hamt with a bit width
    pub fn new_with_bit_width(store: &'a BS, bit_width: u8) -> Self {
        Self {
            root: Node::default(),
            store,
            bit_width,
        }
    }

    /// Lazily instantiate a hamt from this root Cid.
    pub fn load(cid: &Cid, store: &'a BS) -> Result<Self, Error> {
        Self::load_with_bit_width(cid, store, DEFAULT_BIT_WIDTH)
    }

    /// Lazily instantiate a hamt from this root Cid with a specified bit width.
    pub fn load_with_bit_width(cid: &Cid, store: &'a BS, bit_width: u8) -> Result<Self, Error> {
        match store.get(cid)? {
            Some(root) => Ok(Self {
                root,
                store,
                bit_width,
            }),
            None => Err(Error::Custom("No node found")),
        }
    }

    /// Sets the root based on the Cid of the root node using the Hamt store
    pub fn set_root(&mut self, cid: &Cid) -> Result<(), Error> {
        match self.store.get(cid)? {
            Some(root) => self.root = root,
            None => return Err(Error::Custom("No node found")),
        }

        Ok(())
    }

    /// Returns a reference to the underlying store of the Hamt.
    pub fn store(&self) -> &'a BS {
        self.store
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
    /// let mut map: Hamt<usize, _> = Hamt::new(&store);
    /// map.set(37, "a".to_string()).unwrap();
    /// assert_eq!(map.is_empty(), false);
    ///
    /// map.set(37, "b".to_string()).unwrap();
    /// map.set(37, "c".to_string()).unwrap();
    /// ```
    pub fn set<S>(&mut self, key: K, value: S) -> Result<(), Error>
    where
        S: Serialize,
    {
        let val: Ipld = to_ipld(value)?;
        self.root.set(key, val, self.store, self.bit_width)
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
    /// let mut map: Hamt<usize, _> = Hamt::new(&store);
    /// map.set(1, "a".to_string()).unwrap();
    /// assert_eq!(map.get(&1).unwrap(), Some("a".to_string()));
    /// assert_eq!(map.get::<usize, String>(&2).unwrap(), None);
    /// ```
    #[inline]
    pub fn get<Q: ?Sized, V>(&self, k: &Q) -> Result<Option<V>, Error>
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
        V: DeserializeOwned,
    {
        match self.root.get(k, self.store, self.bit_width)? {
            Some(v) => Ok(Some(from_ipld(&v).map_err(Error::Encoding)?)),
            None => Ok(None),
        }
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
    /// let mut map: Hamt<usize, _> = Hamt::new(&store);
    /// map.set(1, "a".to_string()).unwrap();
    /// assert_eq!(map.delete(&1).unwrap(), true);
    /// assert_eq!(map.delete(&1).unwrap(), false);
    /// ```
    pub fn delete<Q: ?Sized>(&mut self, k: &Q) -> Result<bool, Error>
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
    {
        match self.root.remove_entry(k, self.store, self.bit_width)? {
            Some(_) => Ok(true),
            None => Ok(false),
        }
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

    /// Iterates over each KV in the Hamt and runs a function on the values.
    ///
    /// This function will constrain all values to be of the same type
    ///
    /// # Examples
    ///
    /// ```
    /// use ipld_hamt::Hamt;
    ///
    /// let store = db::MemoryDB::default();
    ///
    /// let mut map: Hamt<usize, _> = Hamt::new(&store);
    /// map.set(1, 1).unwrap();
    /// map.set(4, 2).unwrap();
    ///
    /// let mut total = 0;
    /// map.for_each(&mut |_, v: u64| {
    ///    total += v;
    ///    Ok(())
    /// }).unwrap();
    /// assert_eq!(total, 3);
    /// ```
    #[inline]
    pub fn for_each<F, V>(&self, f: &mut F) -> Result<(), String>
    where
        V: DeserializeOwned,
        F: FnMut(&K, V) -> Result<(), String>,
    {
        self.root.for_each(self.store, f)
    }
}
