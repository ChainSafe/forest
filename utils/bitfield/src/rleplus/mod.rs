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

use super::{ranges_from_bits, BitVec, RangeIterator, Result};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

// https://github.com/multiformats/unsigned-varint#practical-maximum-of-9-bytes-for-security
const VARINT_MAX_BYTES: usize = 9;

/// An RLE+ encoded bit field.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct RlePlus(BitVec);

impl Serialize for RlePlus {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serde_bytes::serialize(&self.0.as_slice(), serializer)
    }
}

impl<'de> Deserialize<'de> for RlePlus {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let bytes: Vec<u8> = serde_bytes::deserialize(deserializer)?;
        Self::new(bytes.into()).map_err(serde::de::Error::custom)
    }
}

impl RlePlus {
    /// Creates a new `RlePlus` instance with an already encoded bitvec. Returns an
    /// error if the given bitvec is not RLE+ encoded correctly.
    pub fn new(encoded: BitVec) -> Result<Self> {
        // iterating the runs of the encoded bitvec ensures that it's encoded correctly,
        // and adding the lengths of the runs together ensures that the total length of
        // 1s and 0s fits in a `usize`
        Runs::new(encoded.as_slice())?.try_fold(0_usize, |total_len, run| {
            let (_value, len) = run?;
            total_len.checked_add(len).ok_or("RLE+ overflow")
        })?;
        Ok(Self(encoded))
    }

    /// Encodes the given bitset into its RLE+ encoded representation.
    pub fn encode(raw: &BitVec) -> Self {
        let bits = raw
            .iter()
            .enumerate()
            .filter(|(_, &bit)| bit)
            .map(|(i, _)| i);
        Self::from_ranges(ranges_from_bits(bits))
    }

    /// Decodes an RLE+ encoded bitset into its original form.
    pub fn decode(&self) -> BitVec {
        // the underlying bitvec has already been validated, so nothing here can fail
        let mut bitvec = BitVec::new();
        for run in Runs::new(self.as_bytes()).unwrap() {
            let (value, len) = run.unwrap();
            bitvec.extend(std::iter::repeat(value).take(len));
        }
        bitvec
    }

    /// Returns an iterator over the ranges of 1s of the RLE+ encoded data.
    pub fn ranges(&self) -> Ranges<'_> {
        Ranges::new(self)
    }

    /// Returns `true` if the RLE+ encoded data contains the bit at a given index.
    pub fn get(&self, index: usize) -> bool {
        self.ranges()
            .take_while(|range| range.start < index)
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

        let (bytes, padding_zeros) = writer.finish();
        let mut bitvec = BitVec::from(bytes);

        // `bitvec` now may also contains padding zeros if the number of written
        // bits is not a multiple of 8
        for _ in 0..padding_zeros {
            bitvec.pop();
        }

        // no need to verify, this is valid RLE+ by construction
        Self(bitvec)
    }

    // Returns a byte slice of the bit field's contents.
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_slice()
    }

    // Converts a bit field into a byte vector.
    pub fn into_bytes(self) -> Vec<u8> {
        self.0.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use bitvec::prelude::Lsb0;
    use bitvec::*;
    use rand::{Rng, RngCore, SeedableRng};
    use rand_xorshift::XorShiftRng;

    #[test]
    fn test_rle_plus_basics() {
        let cases: Vec<(BitVec, BitVec)> = vec![
            (
                bitvec![Lsb0, u8; 1; 8],
                bitvec![Lsb0, u8;
                        0, 0, // version
                        1, // starts with 1
                        0, 1, // fits into 4 bits
                        0, 0, 0, 1, // 8 - 1
                ],
            ),
            (
                bitvec![Lsb0, u8; 1, 1, 1, 1, 0, 1, 1, 1],
                bitvec![Lsb0, u8;
                        0, 0, // version
                        1, // starts with 1
                        0, 1, // fits into 4 bits
                        0, 0, 1, 0, // 4 - 1
                        1, // 1 - 0
                        0, 1, // fits into 4 bits
                        1, 1, 0, 0 // 3 - 1
                ],
            ),
            (
                bitvec![Lsb0, u8; 1; 25],
                bitvec![Lsb0, u8;
                        0, 0, // version
                        1, // starts with 1
                        0, 0, // does not fit into 4 bits
                        1, 0, 0, 1, 1, 0, 0, 0 // 25 - 1
                ],
            ),
        ];

        for (i, case) in cases.into_iter().enumerate() {
            assert_eq!(
                RlePlus::encode(&case.0),
                RlePlus::new(case.1.clone()).unwrap(),
                "encoding case {}",
                i
            );
            assert_eq!(
                RlePlus::new(case.1).unwrap().decode(),
                case.0,
                "decoding case: {}",
                i
            );
        }
    }

    #[test]
    fn test_zero_short_block() {
        // decoding should end whenever a length of 0 is encountered

        let encoded = bitvec![Lsb0, u8;
            0, 0, // version
            1, // starts with 1
            1, // 1 - 1
            0, 1, // fits into 4 bits
            0, 0, 0, 0, // 0 - 0
            1, // 1 - 1
        ];

        let decoded = RlePlus::new(encoded).unwrap().decode();
        assert_eq!(decoded, bitvec![Lsb0, u8; 1]);
    }

    fn roundtrip(rng: &mut XorShiftRng, range: usize) {
        let len: usize = rng.gen_range(0, range);

        let mut src = vec![0u8; len];
        rng.fill_bytes(&mut src);

        let mut bitvec = BitVec::from(src);
        while bitvec.last() == Some(&false) {
            bitvec.pop();
        }

        let encoded = RlePlus::encode(&bitvec);
        let decoded = encoded.decode();
        assert_eq!(&bitvec, &decoded);
    }

    #[test]
    #[ignore]
    fn test_rle_plus_roundtrip_small() {
        let mut rng = XorShiftRng::seed_from_u64(1);

        for _i in 0..10_000 {
            roundtrip(&mut rng, 1000);
        }
    }

    #[test]
    #[ignore]
    fn test_rle_plus_roundtrip_large() {
        let mut rng = XorShiftRng::seed_from_u64(2);

        for _i in 0..10_000 {
            roundtrip(&mut rng, 100_000);
        }
    }
}
