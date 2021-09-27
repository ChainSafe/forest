// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod iter;
mod rleplus;
mod unvalidated;

pub use unvalidated::{UnvalidatedBitField, Validate};

use ahash::AHashSet;
use iter::{ranges_from_bits, RangeIterator};
use std::{
    iter::FromIterator,
    ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, BitXor, BitXorAssign, Range, Sub, SubAssign},
};

type Result<T> = std::result::Result<T, &'static str>;

/// A bit field with buffered insertion/removal that serializes to/from RLE+. Similar to
/// `HashSet<usize>`, but more memory-efficient when long runs of 1s and 0s are present.
#[derive(Debug, Default, Clone)]
pub struct BitField {
    /// The underlying ranges of 1s.
    ranges: Vec<Range<usize>>,
    /// Bits set to 1. Never overlaps with `unset`.
    set: AHashSet<usize>,
    /// Bits set to 0. Never overlaps with `set`.
    unset: AHashSet<usize>,
}

impl PartialEq for BitField {
    fn eq(&self, other: &Self) -> bool {
        Iterator::eq(self.ranges(), other.ranges())
    }
}

impl FromIterator<usize> for BitField {
    fn from_iter<I: IntoIterator<Item = usize>>(iter: I) -> Self {
        let mut vec: Vec<_> = iter.into_iter().collect();
        vec.sort_unstable();
        Self::from_ranges(ranges_from_bits(vec))
    }
}

impl FromIterator<bool> for BitField {
    fn from_iter<I: IntoIterator<Item = bool>>(iter: I) -> Self {
        let bits = iter
            .into_iter()
            .enumerate()
            .filter(|&(_, b)| b)
            .map(|(i, _)| i);
        Self::from_ranges(ranges_from_bits(bits))
    }
}

impl BitField {
    /// Creates an empty bit field.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a new bit field from a `RangeIterator`.
    pub fn from_ranges(iter: impl RangeIterator) -> Self {
        Self {
            ranges: iter.collect(),
            ..Default::default()
        }
    }

    /// Adds the bit at a given index to the bit field.
    pub fn set(&mut self, bit: usize) {
        self.unset.remove(&bit);
        self.set.insert(bit);
    }

    /// Removes the bit at a given index from the bit field.
    pub fn unset(&mut self, bit: usize) {
        self.set.remove(&bit);
        self.unset.insert(bit);
    }

    /// Returns `true` if the bit field contains the bit at a given index.
    pub fn get(&self, index: usize) -> bool {
        if self.set.contains(&index) {
            true
        } else if self.unset.contains(&index) {
            false
        } else {
            // since `self.ranges` is ordered, we can use a binary search to find out if
            // any range in `self.ranges` contains `index`
            use std::cmp::Ordering;
            self.ranges
                .binary_search_by(|range| {
                    if index < range.start {
                        Ordering::Greater
                    } else if index >= range.end {
                        Ordering::Less
                    } else {
                        // `index` is contained by this range
                        Ordering::Equal
                    }
                })
                // Ok(range) is returned if the closure returns `Equal` for a certain range,
                // meaning a range in `self.ranges` contains the given index
                .is_ok()
        }
    }

    /// Returns the index of the lowest bit present in the bit field.
    pub fn first(&self) -> Option<usize> {
        // similar to `self.iter.next()`, but optimized using the fact that only the
        // lowest bit in `self.set` is a candidate, and therefore there's no need to
        // sort all bits in `self.set`

        let min_set_bit = self.set.iter().min();

        // turns the `Option<&usize>` minimum set bit into an `Option<Range<usize>>`
        let min_range = min_set_bit.map(|&bit| bit..bit + 1);

        // turns this `Option<Range<usize>>` into a `RangeIterator`, relying on the
        // fact that `Option<T>` is an `IntoIterator` over `T` with 0 or 1 items
        let min_range_iterator = iter::Ranges::new(min_range);

        self.inner_ranges()
            .union(min_range_iterator)
            .flatten()
            .find(|i| !self.unset.contains(i))
    }

    /// Returns an iterator over the indices of the bit field's set bits.
    pub fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        // this code results in the same values as `self.ranges().flatten()`, but there's
        // a key difference:
        //
        // `ranges()` needs to traverse both `self.set` and `self.unset` up front (so before
        // iteration starts) in order to not have to visit each individual bit of `self.bitvec`
        // during iteration, while here we can get away with only traversing `self.set` up
        // front and checking `self.unset` containment for the candidate bits on the fly
        // because we're visiting all bits either way
        //
        // consequently, the time complexity of `self.first()` is only linear in the length of
        // `self.set`, not in the length of `self.unset` (as opposed to getting the first range
        // with `self.ranges().next()` which is linear in both)

        let mut set_bits: Vec<_> = self.set.iter().copied().collect();
        set_bits.sort_unstable();

        self.inner_ranges()
            .union(ranges_from_bits(set_bits))
            .flatten()
            .filter(move |i| !self.unset.contains(i))
    }

    /// Returns an iterator over the indices of the bit field's set bits if the number
    /// of set bits in the bit field does not exceed `max`. Returns an error otherwise.
    pub fn bounded_iter(&self, max: usize) -> Result<impl Iterator<Item = usize> + '_> {
        if self.len() <= max {
            Ok(self.iter())
        } else {
            Err("Bits set exceeds max in retrieval")
        }
    }

    /// Returns an iterator over the ranges without applying the set/unset bits.
    fn inner_ranges(&self) -> impl RangeIterator + '_ {
        iter::Ranges::new(self.ranges.iter().cloned())
    }

    /// Returns an iterator over the ranges of set bits that make up the bit field. The
    /// ranges are in ascending order, are non-empty, and don't overlap.
    pub fn ranges(&self) -> impl RangeIterator + '_ {
        let ranges = |set: &AHashSet<usize>| {
            let mut vec: Vec<_> = set.iter().copied().collect();
            vec.sort_unstable();
            ranges_from_bits(vec)
        };

        self.inner_ranges()
            .union(ranges(&self.set))
            .difference(ranges(&self.unset))
    }

    /// Returns `true` if the bit field is empty.
    pub fn is_empty(&self) -> bool {
        self.set.is_empty()
            && self
                .inner_ranges()
                .flatten()
                .all(|bit| self.unset.contains(&bit))
    }

    /// Returns a slice of the bit field with the start index of set bits
    /// and number of bits to include in the slice. Returns an error if the
    /// bit field contains fewer than `start + len` set bits.
    pub fn slice(&self, start: usize, len: usize) -> Result<Self> {
        let slice = BitField::from_ranges(self.ranges().skip_bits(start).take_bits(len));

        if slice.len() == len {
            Ok(slice)
        } else {
            Err("Not enough bits")
        }
    }

    /// Returns the number of set bits in the bit field.
    pub fn len(&self) -> usize {
        self.ranges().map(|range| range.len()).sum()
    }

    /// Returns a new bit field containing the bits in `self` that remain
    /// after "cutting" out the bits in `other`, and shifting remaining
    /// bits to the left if necessary. For example:
    ///
    /// ```ignore
    /// lhs:     xx-xxx--x
    /// rhs:     -xx-x----
    ///
    /// cut:     x  x x--x
    /// output:  xxx--x
    /// ```
    pub fn cut(&self, other: &Self) -> Self {
        Self::from_ranges(self.ranges().cut(other.ranges()))
    }

    /// Returns the union of the given bit fields as a new bit field.
    pub fn union<'a>(bitfields: impl IntoIterator<Item = &'a Self>) -> Self {
        bitfields.into_iter().fold(Self::new(), |a, b| &a | b)
    }

    /// Returns true if `self` overlaps with `other`.
    pub fn contains_any(&self, other: &BitField) -> bool {
        self.ranges().intersection(other.ranges()).next().is_some()
    }

    /// Returns true if the `self` is a superset of `other`.
    pub fn contains_all(&self, other: &BitField) -> bool {
        other.ranges().difference(self.ranges()).next().is_none()
    }
}

impl BitOr<&BitField> for &BitField {
    type Output = BitField;

    #[inline]
    fn bitor(self, rhs: &BitField) -> Self::Output {
        BitField::from_ranges(self.ranges().union(rhs.ranges()))
    }
}

impl BitOrAssign<&BitField> for BitField {
    #[inline]
    fn bitor_assign(&mut self, rhs: &BitField) {
        *self = &*self | rhs;
    }
}

impl BitAnd<&BitField> for &BitField {
    type Output = BitField;

    #[inline]
    fn bitand(self, rhs: &BitField) -> Self::Output {
        BitField::from_ranges(self.ranges().intersection(rhs.ranges()))
    }
}

impl BitAndAssign<&BitField> for BitField {
    #[inline]
    fn bitand_assign(&mut self, rhs: &BitField) {
        *self = &*self & rhs;
    }
}

impl Sub<&BitField> for &BitField {
    type Output = BitField;

    #[inline]
    fn sub(self, rhs: &BitField) -> Self::Output {
        BitField::from_ranges(self.ranges().difference(rhs.ranges()))
    }
}

impl SubAssign<&BitField> for BitField {
    #[inline]
    fn sub_assign(&mut self, rhs: &BitField) {
        *self = &*self - rhs;
    }
}

impl BitXor<&BitField> for &BitField {
    type Output = BitField;

    fn bitxor(self, rhs: &BitField) -> Self::Output {
        BitField::from_ranges(self.ranges().symmetric_difference(rhs.ranges()))
    }
}

impl BitXorAssign<&BitField> for BitField {
    fn bitxor_assign(&mut self, rhs: &BitField) {
        *self = &*self ^ rhs;
    }
}

/// Constructs a `BitField` from a given list of 1s and 0s.
///
/// # Examples
///
/// ```
/// use forest_bitfield::bitfield;
///
/// let mut bf = bitfield![0, 1, 1, 0, 1, 0, 0, 0, 1, 1];
/// assert!(bf.get(1));
/// assert!(!bf.get(3));
/// bf.set(3);
/// assert_eq!(bf.len(), 6);
/// assert_eq!(bf.ranges().next(), Some(1..5));
/// ```
#[macro_export]
macro_rules! bitfield {
    (@iter) => {
        std::iter::empty::<bool>()
    };
    (@iter $head:literal $(, $tail:literal)*) => {
        std::iter::once($head != 0_u32).chain(bitfield!(@iter $($tail),*))
    };
    ($($val:literal),* $(,)?) => {
        bitfield!(@iter $($val),*).collect::<$crate::BitField>()
    };
}

#[cfg(feature = "json")]
pub mod json {
    use super::*;
    use crate::iter::Ranges;
    use serde::ser::SerializeSeq;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    #[derive(Deserialize, Serialize, Debug, PartialEq)]
    #[serde(transparent)]
    pub struct BitFieldJson(#[serde(with = "self")] pub BitField);

    /// Wrapper for serializing a UnsignedMessage reference to JSON.
    #[derive(Serialize)]
    #[serde(transparent)]
    pub struct BitFieldJsonRef<'a>(#[serde(with = "self")] pub &'a BitField);

    impl From<BitFieldJson> for BitField {
        fn from(wrapper: BitFieldJson) -> Self {
            wrapper.0
        }
    }

    impl From<BitField> for BitFieldJson {
        fn from(wrapper: BitField) -> Self {
            BitFieldJson(wrapper)
        }
    }

    fn serialize<S>(m: &BitField, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let total: usize = m.len();

        if !m.is_empty() {
            let mut seq = serializer.serialize_seq(Some(total))?;
            m.ranges().fold(Ok(0), |last_index, range| {
                let last_index = last_index?;
                let zero_index = (range.start - last_index) as u8;
                let nonzero_index = (range.end - range.start) as u8;
                seq.serialize_element(&zero_index)?;
                seq.serialize_element(&nonzero_index)?;
                Ok(range.end)
            })?;
            seq.end()
        } else {
            let mut seq = serializer.serialize_seq(Some(1))?;
            seq.serialize_element(&0)?;
            seq.end()
        }
    }

    fn deserialize<'de, D>(deserializer: D) -> std::result::Result<BitField, D::Error>
    where
        D: Deserializer<'de>,
    {
        let bitfield_bytes: Vec<usize> = Deserialize::deserialize(deserializer)?;
        let mut ranges: Vec<Range<usize>> = Vec::new();
        bitfield_bytes.iter().fold((false, 0), |last, index| {
            let (should_set, last_index) = last;
            let ending_index = index + last_index;
            if should_set {
                ranges.push(Range {
                    start: last_index,
                    end: ending_index,
                })
            }

            (!should_set, ending_index)
        });
        let ranges = Ranges::new(ranges.iter().cloned());
        Ok(BitField::from_ranges(ranges))
    }

    #[test]
    fn serialization_starts_with_zeros() {
        let bf = BitFieldJson(bitfield![0, 0, 1, 1, 1, 1, 0, 0, 0, 1, 1]);
        let j = serde_json::to_string(&bf).unwrap();
        assert_eq!(j, "[2,4,3,2]");
        let bitfield: BitFieldJson = serde_json::from_str(&j).unwrap();
        assert_eq!(bf, bitfield);
    }

    #[test]
    fn serialization_starts_with_ones() {
        let bf = BitFieldJson(bitfield![1, 1, 1, 1, 1, 1, 0, 0, 0, 1, 1]);
        let j = serde_json::to_string(&bf).unwrap();
        assert_eq!(j, "[0,6,3,2]");
        let bitfield: BitFieldJson = serde_json::from_str(&j).unwrap();
        assert_eq!(bf, bitfield);
    }

    #[test]
    fn serialization_with_single_unut() {
        let bf = BitFieldJson(bitfield![]);
        let j = serde_json::to_string(&bf).unwrap();
        assert_eq!(j, "[0]");
        let bitfield: BitFieldJson = serde_json::from_str(&j).unwrap();
        assert_eq!(bf, bitfield);
    }
}
