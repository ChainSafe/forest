// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use cid::Cid;
use nunny::Vec as NonEmpty;
use serde::{Deserialize, Serialize};

#[cfg(doc)]
use crate::blocks::TipsetKey;

/// There are typically MANY small, immutable collections of CIDs in, e.g [`TipsetKey`]s.
///
/// Save space on those by:
/// - Using [`SmallCid`]s
///   (In the median case, this uses 40 B over 96 B per CID)
///
/// This may be expanded to have [`smallvec`](https://docs.rs/smallvec/1.11.0/smallvec/index.html)-style indirection
/// to save more on heap allocations.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
pub struct SmallCidNonEmptyVec(NonEmpty<SmallCid>);

impl SmallCidNonEmptyVec {
    /// Returns `true` if the slice contains an element with the given value.
    ///
    /// See also [`contains`](https://doc.rust-lang.org/std/primitive.slice.html#method.contains).
    pub fn contains(&self, cid: Cid) -> bool {
        self.0.contains(&SmallCid::from(cid))
    }

    /// Returns a non-empty collection of `CID`
    pub fn into_cids(self) -> NonEmpty<Cid> {
        self.0.into_iter_ne().map(From::from).collect_vec()
    }

    /// Returns an iterator of `CID`s.
    pub fn iter(&self) -> impl Iterator<Item = Cid> + '_ {
        self.0.iter().map(|cid| Cid::from(cid.clone()))
    }

    /// Returns the number of `CID`s
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl<'a> IntoIterator for &'a SmallCidNonEmptyVec {
    type Item = Cid;

    type IntoIter = std::iter::Map<std::slice::Iter<'a, SmallCid>, fn(&SmallCid) -> Cid>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter().map(|cid| Cid::from(cid.clone()))
    }
}

impl IntoIterator for SmallCidNonEmptyVec {
    type Item = Cid;

    type IntoIter =
        std::iter::Map<<NonEmpty<SmallCid> as IntoIterator>::IntoIter, fn(SmallCid) -> Cid>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter().map(Cid::from)
    }
}

/// A [`MaybeCompactedCid`], with indirection to save space on the most common CID variant, at the cost
/// of an extra allocation on rare variants.
///
/// This is NOT intended as a general purpose type - other collections should use the variants
/// of [`MaybeCompactedCid`], so that the discriminant is not repeated.
#[cfg_vis::cfg_vis(doc, pub)]
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SmallCid {
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

#[cfg(test)]
// Note this goes through MaybeCompactedCid, artificially bumping the probability of compact CIDs
impl quickcheck::Arbitrary for SmallCid {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        Self::from(Cid::from(MaybeCompactedCid::arbitrary(g)))
    }
}

impl From<NonEmpty<Cid>> for SmallCidNonEmptyVec {
    fn from(value: NonEmpty<Cid>) -> Self {
        Self(value.into_iter_ne().map(From::from).collect_vec())
    }
}
