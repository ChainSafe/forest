// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::node::Node;
use crate::{Error, Hash};
use cid::Cid;
use ipld_blockstore::BlockStore;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::borrow::Borrow;
use std::ops::Index;

/// Implementation of the HAMT data structure for IPLD.
#[derive(Debug)]
pub struct Hamt<'a, K, V, S: BlockStore> {
    root: Node<K, V>,
    store: &'a S,
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

    /// Removes a key from the HAMT, returning the stored key and value if the
    /// key was previously in the HAMT.
    ///
    /// The key may be any borrowed form of the HAMTS's key type, but
    /// Hash and Eq on the borrowed form *must* match those for
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
    /// map.insert(1, "a".into());
    /// assert_eq!(map.remove_entry(&1), Some((1, "a".into())));
    /// assert_eq!(map.remove(&1), None);
    /// ```
    pub fn remove_entry<Q: ?Sized>(&mut self, k: &Q) -> Option<(K, V)>
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
    {
        self.root.remove_entry(k, self.store)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basics() {
        let store = db::MemoryDB::default();
        let mut hamt = Hamt::new(&store);
        assert!(hamt.insert(1, "world".to_string()).is_none());

        assert_eq!(hamt.get(&1), Some(&"world".to_string()));
        assert_eq!(
            hamt.insert(1, "world2".to_string()),
            Some("world".to_string())
        );
        assert_eq!(hamt.get(&1), Some(&"world2".to_string()));
    }

    #[test]
    fn test_from_link() {
        let store = db::MemoryDB::default();

        let mut hamt: Hamt<usize, String, _> = Hamt::new(&store);
        assert!(hamt.insert(1, "world".to_string()).is_none());

        assert_eq!(hamt.get(&1), Some(&"world".to_string()));
        assert_eq!(
            hamt.insert(1, "world2".to_string()),
            Some("world".to_string())
        );
        assert_eq!(hamt.get(&1), Some(&"world2".to_string()));
        let c = store.put(&hamt.root).unwrap();

        let new_hamt = Hamt::from_link(&c, &store).unwrap();
        assert_eq!(hamt, new_hamt);

        // insert value in the first one
        hamt.insert(2, "stuff".to_string());

        // loading original hash should returnnot be equal now
        let new_hamt = Hamt::from_link(&c, &store).unwrap();
        assert_ne!(hamt, new_hamt);

        // loading new hash
        let c2 = store.put(&hamt.root).unwrap();
        let new_hamt = Hamt::from_link(&c2, &store).unwrap();
        assert_eq!(hamt, new_hamt);

        // loading from an empty store does not work
        let empty_store = db::MemoryDB::default();
        assert!(Hamt::<usize, String, _>::from_link(&c2, &empty_store).is_err());

        // storing the hamt should produce the same cid as storing the root
        let c3 = store.put(&hamt.root).unwrap();
        assert_eq!(c3, c2);
    }
}
