// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use cid::Cid;

#[cfg(doc)]
use std::collections::HashSet;

/// A hash set implemented as a `HashMap` where the value is `()`.
///
/// See also [`HashSet`].
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct CidHashSet {
    inner: CidHashMap<()>,
}

impl CidHashSet {
    /// Creates an empty `HashSet`.
    ///
    /// See also [`HashSet::new`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a value to the set.
    ///
    /// Returns whether the value was newly inserted.
    ///
    /// See also [`HashSet::insert`].
    pub fn insert(&mut self, cid: Cid) -> bool {
        self.inner.insert(cid, ()).is_none()
    }

    /// Returns the number of elements.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns `true` if the set contains a `Cid`.
    #[allow(dead_code)]
    pub fn contains(&self, cid: &Cid) -> bool {
        self.inner.contains_key(cid)
    }

    /// Removes a `Cid` from the set. Returns whether the value was present in the set.
    #[allow(dead_code)]
    pub fn remove(&mut self, cid: &Cid) -> bool {
        self.inner.remove(cid).is_some()
    }

    /// Returns `true` if the set is empty.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

////////////////////
// Collection Ops //
////////////////////

impl Extend<Cid> for CidHashSet {
    fn extend<T: IntoIterator<Item = Cid>>(&mut self, iter: T) {
        self.inner.extend(iter.into_iter().map(|it| (it, ())))
    }
}

impl FromIterator<Cid> for CidHashSet {
    fn from_iter<T: IntoIterator<Item = Cid>>(iter: T) -> Self {
        let mut this = Self::new();
        this.extend(iter);
        this
    }
}
