// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! # RLE+ Bitset Encoding
//!
//! (from https://github.com/filecoin-project/specs/blob/master/src/listings/data_structures.md)
//!
//! RLE+ is a lossless compression format based on [RLE](https://en.wikipedia.org/wiki/Run-length_encoding).
//! Its primary goal is to reduce the size in the case of many individual bits, where RLE breaks down quickly,
//! while keeping the same level of compression for large sets of contiguous bits.
//!
//! In tests it has shown to be more compact than RLE iteself, as well as [Concise](https://arxiv.org/pdf/1004.0403.pdf) and [Roaring](https://roaringbitmap.org/).
//!
//! ## Format
//!
//! The format consists of a header, followed by a series of blocks, of which there are three different types.
//!
//! The format can be expressed as the following [BNF](https://en.wikipedia.org/wiki/Backus%E2%80%93Naur_form) grammar.
//!
//! ```text
//!     <encoding>  ::= <header> <blocks>
//!       <header>  ::= <version> <bit>
//!      <version>  ::= "00"
//!       <blocks>  ::= <block> <blocks> | ""
//!        <block>  ::= <block_single> | <block_short> | <block_long>
//! <block_single>  ::= "1"
//!  <block_short>  ::= "01" <bit> <bit> <bit> <bit>
//!   <block_long>  ::= "00" <unsigned_varint>
//!          <bit>  ::= "0" | "1"
//! ```
//!
//! An `<unsigned_varint>` is defined as specified [here](https://github.com/multiformats/unsigned-varint).
//!
//! ### Header
//!
//! The header indiciates the very first bit of the bit vector to encode. This means the first bit is always
//! the same for the encoded and non encoded form.
//!
//! ### Blocks
//!
//! The blocks represent how many bits, of the current bit type there are. As `0` and `1` alternate in a bit vector
//! the inital bit, which is stored in the header, is enough to determine if a length is currently referencing
//! a set of `0`s, or `1`s.
//!
//! #### Block Single
//!
//! If the running length of the current bit is only `1`, it is encoded as a single set bit.
//!
//! #### Block Short
//!
//! If the running length is less than `16`, it can be encoded into up to four bits, which a short block
//! represents. The length is encoded into a 4 bits, and prefixed with `01`, to indicate a short block.
//!
//! #### Block Long
//!
//! If the running length is `16` or larger, it is encoded into a varint, and then prefixed with `00` to indicate
//! a long block.
//!
//!
//! > **Note:** The encoding is unique, so no matter which algorithm for encoding is used, it should produce
//! > the same encoding, given the same input.
//!

mod iter;
mod reader;
mod writer;

pub use iter::{Ranges, Runs};
use reader::BitReader;
use writer::BitWriter;

use super::{ranges_from_bits, RangeIterator, Result};
use serde::{Deserialize, Deserializer, Serialize};
use std::{fmt, iter::FromIterator};

// https://github.com/multiformats/unsigned-varint#practical-maximum-of-9-bytes-for-security
const VARINT_MAX_BYTES: usize = 9;

/// An RLE+ encoded bit field.
#[derive(Default, Clone, Serialize)]
#[serde(transparent)]
pub struct RlePlus(#[serde(with = "serde_bytes")] Vec<u8>);

impl PartialEq for RlePlus {
    fn eq(&self, other: &Self) -> bool {
        Iterator::eq(self.ranges(), other.ranges())
    }
}

impl<'de> Deserialize<'de> for RlePlus {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let bytes: Vec<u8> = serde_bytes::deserialize(deserializer)?;
        Self::new(bytes).map_err(serde::de::Error::custom)
    }
}

impl FromIterator<usize> for RlePlus {
    fn from_iter<I: IntoIterator<Item = usize>>(iter: I) -> Self {
        let mut vec: Vec<_> = iter.into_iter().collect();
        vec.sort_unstable();
        Self::from_ranges(ranges_from_bits(vec))
    }
}

impl FromIterator<bool> for RlePlus {
    fn from_iter<I: IntoIterator<Item = bool>>(iter: I) -> Self {
        let bits = iter
            .into_iter()
            .enumerate()
            .filter(|&(_, b)| b)
            .map(|(i, _)| i);
        Self::from_ranges(ranges_from_bits(bits))
    }
}

impl fmt::Debug for RlePlus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.ranges()).finish()
    }
}

impl RlePlus {
    /// Creates a new `RlePlus` instance with an already encoded bitvec. Returns an
    /// error if the given bitvec is not RLE+ encoded correctly.
    pub fn new(encoded: Vec<u8>) -> Result<Self> {
        // iterating the runs of the encoded bitvec ensures that it's encoded correctly,
        // and adding the lengths of the runs together ensures that the total length of
        // 1s and 0s fits in a `usize`
        Runs::new(&encoded)?.try_fold(0_usize, |total_len, run| {
            let (_value, len) = run?;
            total_len.checked_add(len).ok_or("RLE+ overflow")
        })?;
        Ok(Self(encoded))
    }

    /// Returns an iterator over the ranges of 1s of the RLE+ encoded data.
    pub fn ranges(&self) -> Ranges<'_> {
        Ranges::new(self)
    }

    /// Returns `true` if the RLE+ encoded data contains the bit at a given index.
    pub fn get(&self, index: usize) -> bool {
        self.ranges()
            .take_while(|range| range.start <= index)
            .any(|range| range.contains(&index))
    }

    /// RLE+ encodes the ranges of 1s from a given `RangeIterator`.
    pub fn from_ranges(mut iter: impl RangeIterator) -> Self {
        let first_range = match iter.next() {
            Some(range) => range,
            None => return Default::default(),
        };

        let mut writer = BitWriter::new();
        writer.write(0, 2); // version 00

        if first_range.start == 0 {
            writer.write(1, 1); // the first bit is a 1
        } else {
            writer.write(0, 1); // the first bit is a 0
            writer.write_len(first_range.start); // the number of leading 0s
        }

        writer.write_len(first_range.len());
        let mut index = first_range.end;

        // for each range of 1s we first encode the number of 0s that came prior
        // before encoding the number of 1s
        for range in iter {
            writer.write_len(range.start - index); // zeros
            writer.write_len(range.len()); // ones
            index = range.end;
        }

        // no need to verify, this is valid RLE+ by construction
        Self(writer.finish())
    }

    /// Returns a byte slice of the bit field's contents.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Converts a bit field into a byte vector.
    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }
}

/// Constructs an `RlePlus` from a given list of 1s and 0s.
///
/// # Examples
///
/// ```
/// use bitfield::rleplus;
///
/// let rleplus = rleplus![0, 1, 1, 0, 1, 0, 0, 0, 1, 1];
/// assert!(rleplus.get(1));
/// assert!(!rleplus.get(3));
/// assert_eq!(rleplus.ranges().next(), Some(1..3));
/// ```
#[macro_export]
macro_rules! rleplus {
    (@iter) => {
        std::iter::empty::<bool>()
    };
    (@iter $head:literal $(, $tail:literal)*) => {
        std::iter::once($head != 0_u32).chain(rleplus!(@iter $($tail),*))
    };
    ($($val:literal),* $(,)?) => {
        rleplus!(@iter $($val),*).collect::<$crate::rleplus::RlePlus>()
    };
}

#[cfg(test)]
mod tests {
    use super::super::{ranges_from_bits, rleplus};
    use super::*;

    use rand::{Rng, SeedableRng};
    use rand_xorshift::XorShiftRng;

    #[test]
    fn test() {
        for (bits, expected) in vec![
            (vec![], rleplus![]),
            (
                vec![
                    0, 0, // version
                    1, // starts with 1
                    0, 1, // fits into 4 bits
                    0, 0, 0, 1, // 8 - 1
                ],
                rleplus![1, 1, 1, 1, 1, 1, 1, 1],
            ),
            (
                vec![
                    0, 0, // version
                    1, // starts with 1
                    0, 1, // fits into 4 bits
                    0, 0, 1, 0, // 4 - 1
                    1, // 1 - 0
                    0, 1, // fits into 4 bits
                    1, 1, 0, 0, // 3 - 1
                ],
                rleplus![1, 1, 1, 1, 0, 1, 1, 1],
            ),
            (
                vec![
                    0, 0, // version
                    1, // starts with 1
                    0, 0, // does not fit into 4 bits
                    1, 0, 0, 1, 1, 0, 0, 0, // 25 - 1
                ],
                rleplus![1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
            ),
            // when a length of 0 is encountered, the rest of the encoded bits should be ignored
            (
                vec![
                    0, 0, // version
                    1, // starts with 1
                    1, // 1 - 1
                    0, 1, // fits into 4 bits
                    0, 0, 0, 0, // 0 - 0
                    1, // 1 - 1
                ],
                rleplus![1],
            ),
        ] {
            let mut writer = BitWriter::new();
            for bit in bits {
                writer.write(bit, 1);
            }
            let rleplus = RlePlus::new(writer.finish()).unwrap();
            assert_eq!(rleplus, expected);
        }
    }

    #[test]
    fn roundtrip() {
        let mut rng = XorShiftRng::seed_from_u64(1);

        for _i in 0..1000 {
            let len: usize = rng.gen_range(0, 1000);
            let bits: Vec<_> = (0..len).filter(|_| rng.gen::<bool>()).collect();

            let ranges: Vec<_> = ranges_from_bits(bits.clone()).collect();
            let rleplus = RlePlus::from_ranges(ranges_from_bits(bits));

            assert_eq!(rleplus.ranges().collect::<Vec<_>>(), ranges);
        }
    }
}
