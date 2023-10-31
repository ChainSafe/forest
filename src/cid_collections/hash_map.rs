// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{CidV1DagCborBlake2b256, MaybeCompactedCid, Uncompactable};
use cid::Cid;
use std::collections::hash_map::{
    Entry as StdEntry, IntoIter as StdIntoIter, OccupiedEntry as StdOccupiedEntry,
    VacantEntry as StdVacantEntry,
};
#[cfg(doc)]
use std::collections::HashMap;

/// A space-optimised hash map of [`Cid`]s, matching the API for [`std::collections::HashMap`].
///
/// We accept the implementation complexity of per-compaction-method `HashMap`s for
/// the space savings, which are constant per-variant, rather than constant per-item.
///
/// This is dramatic for large maps!
/// Using, e.g [`frozen_vec::SmallCid`](super::frozen_vec::SmallCid) will cost
/// 25% more per-CID in the median case (32 B vs 40 B)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CidHashMap<V> {
    compact: ahash::HashMap<CidV1DagCborBlake2b256, V>,
    uncompact: ahash::HashMap<Uncompactable, V>,
}

impl<V> CidHashMap<V> {
    /// Creates an empty `HashMap`.
    ///
    /// See also [`HashMap::new`].
    pub fn new() -> Self {
        Self::default()
    }
    /// Returns the number of elements in the map.
    ///
    /// See also [`HashMap::len`].
    pub fn len(&self) -> usize {
        let Self { compact, uncompact } = self;
        compact.len() + uncompact.len()
    }
    /// How many values this map is guaranteed to hold without reallocating.
    #[allow(dead_code)] // mirror of `total_capacity`, below
    pub fn capacity_min(&self) -> usize {
        let Self { compact, uncompact } = self;
        std::cmp::min(compact.capacity(), uncompact.capacity())
    }
    /// Reflective of reserved capacity of this map.
    pub fn total_capacity(&self) -> usize {
        let Self { compact, uncompact } = self;
        compact.capacity() + uncompact.capacity()
    }
    /// Returns `true` if the map contains a value for the specified key.
    ///
    /// See also [`HashMap::contains_key`].
    pub fn contains_key(&self, key: &Cid) -> bool {
        match MaybeCompactedCid::from(*key) {
            MaybeCompactedCid::Compact(c) => self.compact.contains_key(&c),
            MaybeCompactedCid::Uncompactable(u) => self.uncompact.contains_key(&u),
        }
    }
    /// Returns a reference to the value corresponding to the key.
    ///
    /// See also [`HashMap::get`].
    pub fn get(&self, key: &Cid) -> Option<&V> {
        match MaybeCompactedCid::from(*key) {
            MaybeCompactedCid::Compact(c) => self.compact.get(&c),
            MaybeCompactedCid::Uncompactable(u) => self.uncompact.get(&u),
        }
    }
    /// Inserts a key-value pair into the map.
    ///
    /// If the map did not have this key present, [`None`] is returned.
    ///
    /// If the map did have this key present, the value is updated, and the old
    /// value is returned.
    ///
    /// See also [`HashMap::insert`].
    pub fn insert(&mut self, key: Cid, value: V) -> Option<V> {
        match MaybeCompactedCid::from(key) {
            MaybeCompactedCid::Compact(c) => self.compact.insert(c, value),
            MaybeCompactedCid::Uncompactable(u) => self.uncompact.insert(u, value),
        }
    }
    /// Removes a key from the map, returning the value at the key if the key
    /// was previously in the map.
    ///
    /// See also [`HashMap::remove`].
    pub fn remove(&mut self, key: &Cid) -> Option<V> {
        match MaybeCompactedCid::from(*key) {
            MaybeCompactedCid::Compact(c) => self.compact.remove(&c),
            MaybeCompactedCid::Uncompactable(u) => self.uncompact.remove(&u),
        }
    }
}

///////////////
// Entry API //
///////////////

impl<V> CidHashMap<V> {
    /// Gets the given key's corresponding entry in the map for in-place manipulation.
    ///
    /// See also [`HashMap::entry`].
    pub fn entry(&mut self, key: Cid) -> Entry<V> {
        match MaybeCompactedCid::from(key) {
            MaybeCompactedCid::Compact(c) => match self.compact.entry(c) {
                StdEntry::Occupied(o) => Entry::Occupied(OccupiedEntry {
                    inner: OccupiedEntryInner::Compact(o),
                }),
                StdEntry::Vacant(v) => Entry::Vacant(VacantEntry {
                    inner: VacantEntryInner::Compact(v),
                }),
            },
            MaybeCompactedCid::Uncompactable(u) => match self.uncompact.entry(u) {
                StdEntry::Occupied(o) => Entry::Occupied(OccupiedEntry {
                    inner: OccupiedEntryInner::Uncompact(o),
                }),
                StdEntry::Vacant(v) => Entry::Vacant(VacantEntry {
                    inner: VacantEntryInner::Uncompact(v),
                }),
            },
        }
    }
}

/// A view into a single entry in a map, which may either be vacant or occupied.
///
/// This `enum` is constructed using [`CidHashMap::entry`].
#[derive(Debug)]
pub enum Entry<'a, V: 'a> {
    /// An occupied entry.
    Occupied(OccupiedEntry<'a, V>),
    /// A vacant entry.
    Vacant(VacantEntry<'a, V>),
}

/// A view into an occupied entry in a `HashMap`.
/// It is part of the [`Entry`] enum.
///
/// See also [`std::collections::hash_map::OccupiedEntry`].
#[derive(Debug)]
pub struct OccupiedEntry<'a, V> {
    inner: OccupiedEntryInner<'a, V>,
}

impl<'a, V> OccupiedEntry<'a, V> {
    /// Gets a reference to the value in the entry.
    ///
    /// See also [`std::collections::hash_map::OccupiedEntry::get`].
    pub fn get(&self) -> &V {
        match &self.inner {
            OccupiedEntryInner::Compact(c) => c.get(),
            OccupiedEntryInner::Uncompact(u) => u.get(),
        }
    }
}

/// Hides compaction from users.
#[derive(Debug)]
enum OccupiedEntryInner<'a, V> {
    Compact(StdOccupiedEntry<'a, CidV1DagCborBlake2b256, V>),
    Uncompact(StdOccupiedEntry<'a, Uncompactable, V>),
}

/// A view into a vacant entry in a `HashMap`.
/// It is part of the [`Entry`] enum.
///
/// See also [`std::collections::hash_map::VacantEntry`].
#[derive(Debug)]
pub struct VacantEntry<'a, V> {
    inner: VacantEntryInner<'a, V>,
}

impl<'a, V> VacantEntry<'a, V> {
    /// Sets the value of the entry with the `VacantEntry`'s key,
    /// and returns a mutable reference to it.
    ///
    /// See also [`std::collections::hash_map::VacantEntry::insert`].
    pub fn insert(self, value: V) -> &'a mut V {
        match self.inner {
            VacantEntryInner::Compact(c) => c.insert(value),
            VacantEntryInner::Uncompact(u) => u.insert(value),
        }
    }
}

/// Hides compaction from users.
#[derive(Debug)]
enum VacantEntryInner<'a, V> {
    Compact(StdVacantEntry<'a, CidV1DagCborBlake2b256, V>),
    Uncompact(StdVacantEntry<'a, Uncompactable, V>),
}

////////////////////
// Collection Ops //
////////////////////

impl<V> Default for CidHashMap<V> {
    fn default() -> Self {
        Self {
            compact: Default::default(),
            uncompact: Default::default(),
        }
    }
}

impl<V> Extend<(Cid, V)> for CidHashMap<V> {
    fn extend<T: IntoIterator<Item = (Cid, V)>>(&mut self, iter: T) {
        for (cid, v) in iter {
            match MaybeCompactedCid::from(cid) {
                MaybeCompactedCid::Compact(compact) => {
                    self.compact.insert(compact, v);
                }
                MaybeCompactedCid::Uncompactable(uncompact) => {
                    self.uncompact.insert(uncompact, v);
                }
            };
        }
    }
}

impl<V> FromIterator<(Cid, V)> for CidHashMap<V> {
    fn from_iter<T: IntoIterator<Item = (Cid, V)>>(iter: T) -> Self {
        let mut this = Self::new();
        this.extend(iter);
        this
    }
}

pub struct IntoIter<V> {
    compact: StdIntoIter<CidV1DagCborBlake2b256, V>,
    uncompact: StdIntoIter<Uncompactable, V>,
}

impl<V> Iterator for IntoIter<V> {
    type Item = (Cid, V);

    fn next(&mut self) -> Option<Self::Item> {
        self.compact
            .next()
            .map(|(k, v)| (MaybeCompactedCid::Compact(k).into(), v))
            .or_else(|| {
                self.uncompact
                    .next()
                    .map(|(k, v)| (MaybeCompactedCid::Uncompactable(k).into(), v))
            })
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        join_size_hints(self.compact.size_hint(), self.uncompact.size_hint())
    }
}

fn join_size_hints(
    left: (usize, Option<usize>),
    right: (usize, Option<usize>),
) -> (usize, Option<usize>) {
    let (l_lower, l_upper) = left;
    let (r_lower, r_upper) = right;
    let lower = l_lower.saturating_add(r_lower);
    let upper = match (l_upper, r_upper) {
        (Some(l), Some(r)) => l.checked_add(r),
        _ => None,
    };
    (lower, upper)
}

impl<V> IntoIterator for CidHashMap<V> {
    type Item = (Cid, V);

    type IntoIter = IntoIter<V>;

    fn into_iter(self) -> Self::IntoIter {
        let Self { compact, uncompact } = self;
        // required for contract of ExactSizeIterator
        assert!(compact.len().checked_add(uncompact.len()).is_some());
        IntoIter {
            compact: compact.into_iter(),
            uncompact: uncompact.into_iter(),
        }
    }
}

impl<V> ExactSizeIterator for IntoIter<V> {}

//////////
// Keys //
//////////

#[cfg(test)]
use std::collections::hash_map::Keys as StdKeys;

#[cfg(test)]
impl<V> CidHashMap<V> {
    /// An iterator visiting all keys in arbitrary order.
    ///
    /// In a notable departure from [`HashMap::keys`], the element type is [`Cid`], not [`&Cid`].
    ///
    pub fn keys(&self) -> Keys<'_, V> {
        let Self { compact, uncompact } = self;
        Keys {
            compact: compact.keys(),
            uncompact: uncompact.keys(),
        }
    }
}

/// An iterator over the keys of a `HashMap`.
///
/// See [`CidHashMap::keys`].
#[cfg(test)]
pub struct Keys<'a, V> {
    compact: StdKeys<'a, CidV1DagCborBlake2b256, V>,
    uncompact: StdKeys<'a, Uncompactable, V>,
}

#[cfg(test)]
impl<'a, V> Iterator for Keys<'a, V> {
    type Item = Cid;

    fn next(&mut self) -> Option<Self::Item> {
        self.compact
            .next()
            .copied()
            .map(MaybeCompactedCid::Compact)
            .map(Into::into)
            .or_else(|| {
                self.uncompact
                    .next()
                    .copied()
                    .map(MaybeCompactedCid::Uncompactable)
                    .map(Into::into)
            })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        join_size_hints(self.compact.size_hint(), self.uncompact.size_hint())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use quickcheck::quickcheck;

    #[derive(derive_quickcheck_arbitrary::Arbitrary, Clone, Debug)]
    enum Operation {
        ContainsKey(MaybeCompactedCid),
        Get(MaybeCompactedCid),
        Insert(MaybeCompactedCid, u8),
        Entry { key: MaybeCompactedCid, value: u8 },
    }

    quickcheck! {
        fn operations(operations: Vec<Operation>) -> () {
            use Operation as Op;

            let mut subject = CidHashMap::default();
            let mut reference = ahash::HashMap::default();
            for operation in operations {
                match operation {
                    Op::ContainsKey(key) => {
                        let key = key.into();
                        assert_eq!(
                            subject.contains_key(&key),
                            reference.contains_key(&key)
                        )
                    },
                    Op::Get(key) => {
                        let key = key.into();
                        assert_eq!(
                            subject.get(&key),
                            reference.get(&key)
                        )
                    },
                    Op::Insert(key, val) => {
                        let key = key.into();
                        assert_eq!(
                            subject.insert(key, val),
                            reference.insert(key, val)
                        )
                    },
                    Op::Entry {
                        key, value
                    } => {
                        let key = key.into();
                        match (subject.entry(key), reference.entry(key)) {
                            (Entry::Occupied(subj), StdEntry::Occupied(refr)) => assert_eq!(subj.get(), refr.get()),
                            (Entry::Vacant(subj), StdEntry::Vacant(refr)) => assert_eq!(subj.insert(value), refr.insert(value)),
                            (subj, refr) => panic!("{subj:?}, {refr:?}")
                        }
                    }
                }
            };
            assert_eq!(reference, ahash::HashMap::from_iter(subject));
        }

        fn collect(pairs: Vec<(Cid, u8)>) -> () {
            let refr = ahash::HashMap::from_iter(pairs.clone());
            let via_subject = ahash::HashMap::from_iter(
                CidHashMap::from_iter(pairs)
            );
            assert_eq!(refr, via_subject);
        }
    }
}
