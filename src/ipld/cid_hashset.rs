// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::utils::cid::{CidVariant, BLAKE2B256_SIZE};
use ahash::{HashMap, HashMapExt};
use cid::Cid;

// The size of a CID is 96 bytes. A CID contains:
//   - a version
//   - a codec
//   - a hash code
//   - a length
//   - 64 bytes pre-allocated buffer
// Each non-buffer field takes 8 bytes with padding. So, 4*8 = 32 bytes, 32 + 64 = 96 bytes.
//
// However, we know that nearly all Filecoin CIDs have version=V1, codec=DAG_CBOR, code=Blake2b and
// length=32. Taking advantage of this knowledge, we can store the vast majority of CIDs (+99.99%)
// in one third of the usual space (32 bytes vs 96 bytes).
#[derive(Default)]
pub struct CidHashSet(CidHashMap<()>);

impl CidHashSet {
    /// Adds a value to the set if not already present and returns whether the value was newly inserted.
    pub fn insert(&mut self, cid: Cid) -> bool {
        if self.0.contains_key(cid) {
            false
        } else {
            self.0.insert(cid, ()).is_none()
        }
    }

    /// Returns the number of items in the set.
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

#[derive(Debug, Default)]
pub struct CidHashMap<V> {
    v1_dagcbor_blake2b_hash_map: HashMap<[u8; BLAKE2B256_SIZE], V>,
    fallback_hash_map: HashMap<Cid, V>,
}

impl<V> CidHashMap<V> {
    /// Creates an empty `HashMap` with CID type keys.
    pub fn new() -> Self {
        Self {
            v1_dagcbor_blake2b_hash_map: HashMap::new(),
            fallback_hash_map: HashMap::new(),
        }
    }

    /// Returns `true` if the map contains a value for the specified key.
    pub fn contains_key(&self, k: Cid) -> bool {
        match k.try_into() {
            Ok(CidVariant::V1DagCborBlake2b(bytes)) => {
                self.v1_dagcbor_blake2b_hash_map.contains_key(&bytes)
            }
            Err(()) => self.fallback_hash_map.contains_key(&k),
        }
    }

    /// Inserts a key-value pair into the map; if the map did not have this key present, [`None`] is returned.
    pub fn insert(&mut self, k: Cid, v: V) -> Option<V> {
        match k.try_into() {
            Ok(CidVariant::V1DagCborBlake2b(bytes)) => {
                self.v1_dagcbor_blake2b_hash_map.insert(bytes, v)
            }
            Err(()) => self.fallback_hash_map.insert(k, v),
        }
    }

    /// Removes a key from the map, returning the value at the key if the key
    /// was previously in the map.
    pub fn remove(&mut self, k: Cid) -> Option<V> {
        match k.try_into() {
            Ok(CidVariant::V1DagCborBlake2b(bytes)) => {
                self.v1_dagcbor_blake2b_hash_map.remove(&bytes)
            }
            Err(()) => self.fallback_hash_map.remove(&k),
        }
    }

    /// Returns the number of elements the map can hold without reallocating.
    pub fn capacity(&self) -> usize {
        self.v1_dagcbor_blake2b_hash_map.capacity() + self.fallback_hash_map.capacity()
    }

    /// Returns a reference to the value corresponding to the key.
    pub fn get(&self, k: Cid) -> Option<&V> {
        match k.try_into() {
            Ok(CidVariant::V1DagCborBlake2b(bytes)) => self.v1_dagcbor_blake2b_hash_map.get(&bytes),
            Err(()) => self.fallback_hash_map.get(&k),
        }
    }

    /// Returns the number of elements in the map.
    pub fn len(&self) -> usize {
        self.v1_dagcbor_blake2b_hash_map.len() + self.fallback_hash_map.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quickcheck_macros::quickcheck;

    fn quickcheck_constructor(cid: Cid, payload: u64) -> (CidHashMap<u64>, HashMap<Cid, u64>) {
        let mut cid_hash_map = CidHashMap::new();
        let mut hash_map = HashMap::new();
        cid_hash_map.insert(cid, payload);
        hash_map.insert(cid, payload);
        (cid_hash_map, hash_map)
    }

    #[quickcheck]
    fn insert_key(cid: Cid, payload: u64) {
        let mut cid_hash_map = CidHashMap::new();
        let mut hash_map = HashMap::new();
        assert_eq!(
            cid_hash_map.insert(cid, payload),
            hash_map.insert(cid, payload)
        );
    }

    #[quickcheck]
    fn contains_key(cid: Cid, payload: u64) {
        let (cid_hash_map, hash_map) = quickcheck_constructor(cid, payload);
        assert_eq!(cid_hash_map.contains_key(cid), hash_map.contains_key(&cid));
    }

    #[quickcheck]
    fn remove_key(cid: Cid, payload: u64) {
        let (mut cid_hash_map, mut hash_map) = quickcheck_constructor(cid, payload);
        assert_eq!(cid_hash_map.remove(cid), hash_map.remove(&cid));
    }

    #[quickcheck]
    fn get_value_at_key(cid: Cid, payload: u64) {
        let (cid_hash_map, hash_map) = quickcheck_constructor(cid, payload);
        assert_eq!(cid_hash_map.get(cid), hash_map.get(&cid));
    }

    #[quickcheck]
    fn len(cid: Cid, payload: u64) {
        let (cid_hash_map, hash_map) = quickcheck_constructor(cid, payload);
        assert_eq!(cid_hash_map.len(), hash_map.len());
    }

    #[quickcheck]
    fn capacity(cid: Cid, payload: u64) {
        let (cid_hash_map, hash_map) = quickcheck_constructor(cid, payload);
        assert_eq!(cid_hash_map.capacity(), hash_map.capacity());
    }
}
