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

    /// Returns the number of elements in the set.
    ///
    /// See also [`HashSet::len`].
    pub fn len(&self) -> usize {
        self.inner.len()
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

pub struct IntoIter {
    inner: hash_map::IntoIter<()>,
}

impl Iterator for IntoIter {
    type Item = Cid;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|(it, ())| it)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl IntoIterator for CidHashSet {
    type Item = Cid;

    type IntoIter = IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        IntoIter {
            inner: self.inner.into_iter(),
        }
    }
}
