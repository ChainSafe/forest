// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{CidV1DagCborBlake2b256, MaybeCompactedCid, Uncompactable};
use cid::Cid;
use std::collections::hash_map::{
    Entry as StdEntry, IntoIter as StdIntoIter, OccupiedEntry as StdOccupiedEntry,
    VacantEntry as StdVacantEntry,
};

/// A space-optimised hashmap of [`Cid`]s
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CidHashMap<V> {
    compact: ahash::HashMap<CidV1DagCborBlake2b256, V>,
    uncompact: ahash::HashMap<Uncompactable, V>,
}

impl<V> CidHashMap<V> {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn len(&self) -> usize {
        let Self { compact, uncompact } = self;
        compact.len() + uncompact.len()
    }
    /// How many values this map could hold without reallocating
    #[allow(dead_code)] // mirror of `total_capacity`, below
    pub fn capacity_min(&self) -> usize {
        let Self { compact, uncompact } = self;
        std::cmp::min(compact.capacity(), uncompact.capacity())
    }
    /// Reflective of memory usage of this map
    pub fn total_capacity(&self) -> usize {
        let Self { compact, uncompact } = self;
        compact.capacity() + uncompact.capacity()
    }
    pub fn contains_key(&self, key: &Cid) -> bool {
        match MaybeCompactedCid::from(*key) {
            MaybeCompactedCid::Compact(c) => self.compact.contains_key(&c),
            MaybeCompactedCid::Uncompactable(u) => self.uncompact.contains_key(&u),
        }
    }
    pub fn get(&self, key: &Cid) -> Option<&V> {
        match MaybeCompactedCid::from(*key) {
            MaybeCompactedCid::Compact(c) => self.compact.get(&c),
            MaybeCompactedCid::Uncompactable(u) => self.uncompact.get(&u),
        }
    }
    pub fn insert(&mut self, key: Cid, value: V) -> Option<V> {
        match MaybeCompactedCid::from(key) {
            MaybeCompactedCid::Compact(c) => self.compact.insert(c, value),
            MaybeCompactedCid::Uncompactable(u) => self.uncompact.insert(u, value),
        }
    }
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

#[derive(Debug)]
pub enum Entry<'a, V: 'a> {
    /// An occupied entry.
    Occupied(OccupiedEntry<'a, V>),
    /// A vacant entry.
    Vacant(VacantEntry<'a, V>),
}

#[derive(Debug)]
pub struct OccupiedEntry<'a, V> {
    inner: OccupiedEntryInner<'a, V>,
}

impl<'a, V> OccupiedEntry<'a, V> {
    pub fn get(&self) -> &V {
        match &self.inner {
            OccupiedEntryInner::Compact(c) => c.get(),
            OccupiedEntryInner::Uncompact(u) => u.get(),
        }
    }
}

#[derive(Debug)]
enum OccupiedEntryInner<'a, V> {
    Compact(StdOccupiedEntry<'a, CidV1DagCborBlake2b256, V>),
    Uncompact(StdOccupiedEntry<'a, Uncompactable, V>),
}

#[derive(Debug)]
pub struct VacantEntry<'a, V> {
    inner: VacantEntryInner<'a, V>,
}

impl<'a, V> VacantEntry<'a, V> {
    pub fn insert(self, value: V) -> &'a mut V {
        match self.inner {
            VacantEntryInner::Compact(c) => c.insert(value),
            VacantEntryInner::Uncompact(u) => u.insert(value),
        }
    }
}

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
}

impl<V> IntoIterator for CidHashMap<V> {
    type Item = (Cid, V);

    type IntoIter = IntoIter<V>;

    fn into_iter(self) -> Self::IntoIter {
        let Self { compact, uncompact } = self;
        IntoIter {
            compact: compact.into_iter(),
            uncompact: uncompact.into_iter(),
        }
    }
}

//////////
// Keys //
//////////

#[cfg(test)]
use std::collections::hash_map::Keys as StdKeys;

#[cfg(test)]
impl<V> CidHashMap<V> {
    pub fn keys(&self) -> Keys<'_, V> {
        let Self { compact, uncompact } = self;
        Keys {
            compact: compact.keys(),
            uncompact: uncompact.keys(),
        }
    }
}

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
