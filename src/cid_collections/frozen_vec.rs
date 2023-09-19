use super::*;
use cid::Cid;
use serde::{Deserialize, Serialize};

#[cfg(doc)]
use crate::blocks::TipsetKeys;

/// There are typically MANY small, immutable collections of CIDs in, e.g [`TipsetKeys`].
///
/// Save space on those by:
/// - Using a boxed slice to save on vector overallocation.
///   (In the worst case, this uses half the memory)
/// - Using [`SmallCid`]s
///   (In the median case, this uses 40 B over 96 B per CID)
///
/// This may be expanded to have [`smallvec`](https://docs.rs/smallvec/1.11.0/smallvec/index.html)-style indirection
/// to save more on heap allocations.
#[derive(Default, Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FrozenCidVec {
    inner: Box<[SmallCid]>,
}

impl FrozenCidVec {
    /// Returns true if the slice has a length of 0.
    ///
    /// See [`core::slice::is_empty`].
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
    /// Returns `true` if the slice contains an element with the given value.
    ///
    /// See [`core::slice::contains`].
    pub fn contains(&self, cid: Cid) -> bool {
        self.inner.contains(&SmallCid::from(cid))
    }
}

/// A [`MaybeCompactedCid`], with indirection to save space on the most common CID variant, at the cost
/// of an extra allocation on rare variants.
///
/// This is NOT intended as a general purpose type - other collections should use the variants
/// of [`MaybeCompactedCid`], so that the discriminant is not repeated.
#[cfg_vis::cfg_vis(doc, pub)]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum SmallCid {
    Inline(CidV1DagCborBlake2b256),
    Indirect(Box<Uncompactable>),
}

//////////////////////////
// SmallCid conversions //
//////////////////////////

impl From<Cid> for SmallCid {
    fn from(value: Cid) -> Self {
        match MaybeCompactedCid::from(value) {
            MaybeCompactedCid::Compact(c) => Self::Inline(c),
            MaybeCompactedCid::Uncompactable(u) => Self::Indirect(Box::new(u)),
        }
    }
}

impl From<SmallCid> for Cid {
    fn from(value: SmallCid) -> Self {
        match value {
            SmallCid::Inline(c) => c.into(),
            SmallCid::Indirect(u) => (*u).into(),
        }
    }
}

impl Serialize for SmallCid {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        Cid::from(self.clone()).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SmallCid {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Cid::deserialize(deserializer).map(Into::into)
    }
}

/////////////////////
// Arbitrary impls //
/////////////////////
// Note these go through MaybeCompactedCid, artificially bumping the probability of compact CIDs

#[cfg(test)]
impl quickcheck::Arbitrary for SmallCid {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        Self::from(Cid::from(MaybeCompactedCid::arbitrary(g)))
    }
}

#[cfg(test)]
impl quickcheck::Arbitrary for FrozenCidVec {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        Vec::<MaybeCompactedCid>::arbitrary(g)
            .into_iter()
            .map(Cid::from)
            .collect()
    }
}

/////////////////////////////////
// FrozenCidVec collection Ops //
/////////////////////////////////

impl FromIterator<Cid> for FrozenCidVec {
    fn from_iter<T: IntoIterator<Item = Cid>>(iter: T) -> Self {
        Self {
            inner: iter.into_iter().map(SmallCid::from).collect(),
        }
    }
}

pub struct IntoIter {
    inner: std::vec::IntoIter<SmallCid>,
}

impl Iterator for IntoIter {
    type Item = Cid;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(Into::into)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl IntoIterator for FrozenCidVec {
    type Item = Cid;

    type IntoIter = IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        IntoIter {
            inner: self.inner.into_vec().into_iter(),
        }
    }
}
