use crate::bitfield::Bitfield;
use crate::hash::Hash;
use crate::Error;
use cid::Cid;
use forest_encoding::{de::Deserializer, ser::Serializer};
use ipld_blockstore::BlockStore;
use lazycell::AtomicLazyCell;
use murmur3::murmur3_x64_128::MurmurHasher;
use replace_with::replace_with;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::borrow::Borrow;
use std::ops::Index;

const MAX_ARRAY_WIDTH: usize = 3;

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

#[derive(Debug, Clone, PartialEq, Eq)]
struct Node<K, V> {
    bitfield: Bitfield,
    pointers: Vec<Pointer<K, V>>,
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

type HashedKey = [u8; 16];

// TODO: make Pointer an enum once things are working
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(bound(deserialize = "KeyValuePair<K, V>: DeserializeOwned"))]
struct Pointer<K, V> {
    #[serde(rename = "v", skip_serializing_if = "Vec::is_empty")]
    kvs: Vec<KeyValuePair<K, V>>,
    #[serde(rename = "l", skip_serializing_if = "Option::is_none")]
    link: Option<Cid>,
    #[serde(skip)]
    cache: AtomicLazyCell<Node<K, V>>,
}

impl<K: PartialEq, V: PartialEq> PartialEq for Pointer<K, V> {
    fn eq(&self, other: &Self) -> bool {
        self.kvs == other.kvs && self.link == other.link
    }
}

impl<K: Eq, V: Eq> Eq for Pointer<K, V> {}

impl<K, V> Default for Pointer<K, V> {
    fn default() -> Self {
        Pointer {
            kvs: Vec::new(),
            link: None,
            cache: AtomicLazyCell::new(),
        }
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct KeyValuePair<K, V>(K, V);

impl<K, V> KeyValuePair<K, V> {
    pub fn key(&self) -> &K {
        &self.0
    }

    pub fn value(&self) -> &V {
        &self.1
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
            None => Err(Error::Custom("No node found".to_owned())),
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

// TODO: should all operations be wrapped into Result?

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
            return Err(Error::Custom("Maximum depth reached".to_owned()));
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
            return Err(Error::Custom("Maximum depth reached".to_owned()));
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
        let res = mask.and(&self.bitfield).count_ones();
        res
    }

    fn get_child_mut(&mut self, i: usize) -> &mut Pointer<K, V> {
        &mut self.pointers[i]
    }

    fn get_child<'a>(&'a self, i: usize) -> &'a Pointer<K, V> {
        &self.pointers[i]
    }
}

impl<K, V> KeyValuePair<K, V> {
    pub fn new(key: K, value: V) -> Self {
        KeyValuePair(key, value)
    }
}

impl<K, V> Pointer<K, V>
where
    K: Serialize + DeserializeOwned,
    V: Serialize + DeserializeOwned,
{
    pub fn from_link(link: Cid, node: Node<K, V>) -> Self {
        let cache = AtomicLazyCell::new();
        cache.fill(node).map_err(|_| ()).unwrap();

        Pointer {
            kvs: Vec::new(),
            link: Some(link),
            cache,
        }
    }

    pub fn from_key_value(key: K, value: V) -> Self {
        Pointer {
            kvs: vec![KeyValuePair::new(key, value)],
            link: None,
            cache: AtomicLazyCell::new(),
        }
    }

    pub fn from_kvpairs(kvs: Vec<KeyValuePair<K, V>>) -> Self {
        Pointer {
            kvs,
            link: None,
            cache: AtomicLazyCell::new(),
        }
    }

    pub fn is_shard(&self) -> bool {
        self.link.is_some()
    }

    fn load_child<S: BlockStore>(&self, store: &S) -> Result<&Node<K, V>, Error> {
        if !self.cache.filled() {
            if let Some(ref link) = self.link {
                match store.get(link)? {
                    Some(node) => {
                        self.cache.fill(node).map_err(|_| ()).unwrap();
                    }
                    None => return Err(Error::Custom("node not found".to_owned())),
                }
            } else {
                return Err(Error::Custom(
                    "Cannot load child from non link node".to_owned(),
                ));
            }
        }
        Ok(self.cache.borrow().unwrap())
    }

    fn load_child_mut<S: BlockStore>(&mut self, store: &S) -> Result<&mut Node<K, V>, Error> {
        if !self.cache.filled() {
            if let Some(ref link) = self.link {
                match store.get(link)? {
                    Some(node) => {
                        self.cache.fill(node).map_err(|_| ()).unwrap();
                    }
                    None => return Err(Error::Custom("node not found".to_owned())),
                }
            } else {
                return Err(Error::Custom(
                    "Cannot load child from non link node".to_owned(),
                ));
            }
        }
        Ok(self.cache.borrow_mut().unwrap())
    }

    /// Internal method to cleanup children, to ensure consisten tree representation
    /// after deletes.
    pub fn clean(&mut self) -> Result<(), Error> {
        assert!(self.cache.filled());
        let len = self.cache.borrow().unwrap().pointers.len();
        if len <= 0 {
            return Err(Error::Custom("Invalid HAMT".to_owned()));
        }

        replace_with(
            self,
            || panic!(),
            |self_| {
                match len {
                    1 => {
                        // TODO: investigate todo in go-hamt-ipld
                        if self_.cache.borrow().unwrap().pointers[0].is_shard() {
                            return self_;
                        }

                        self_
                            .cache
                            .into_inner()
                            .unwrap()
                            .pointers
                            .into_iter()
                            .nth(0)
                            .unwrap()
                    }
                    1..=MAX_ARRAY_WIDTH => {
                        let (total_lens, has_shards): (Vec<_>, Vec<_>) = self_
                            .cache
                            .borrow()
                            .unwrap()
                            .pointers
                            .iter()
                            .map(|p| (p.kvs.len(), p.is_shard()))
                            .unzip();

                        let total_len: usize = total_lens.iter().sum();
                        let has_shards = has_shards.into_iter().any(|a| a);

                        if total_len >= MAX_ARRAY_WIDTH || has_shards {
                            return self_;
                        }

                        let chvals = self_
                            .cache
                            .into_inner()
                            .unwrap()
                            .pointers
                            .into_iter()
                            .map(|p| p.kvs)
                            .flatten()
                            .collect();

                        Pointer::from_kvpairs(chvals)
                    }
                    _ => self_,
                }
            },
        );
        Ok(())
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
