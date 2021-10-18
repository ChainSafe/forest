// Copyright 2019-2022 ChainSafe Systems
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

mod reader;
mod writer;

pub use reader::BitReader;
pub use writer::BitWriter;

use super::{BitField, Result};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::borrow::Cow;

// MaxEncodedSize is the maximum encoded size of a bitfield. When expanded into
// a slice of runs, a bitfield of this size should not exceed 2MiB of memory.
//
// This bitfield can fit at least 3072 sparse elements.
const MAX_ENCODED_SIZE: usize = 32 << 10;

impl Serialize for BitField {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let bytes = self.to_bytes();
        if bytes.len() > MAX_ENCODED_SIZE {
            return Err(serde::ser::Error::custom(format!(
                "encoded bitfield was too large {}",
                bytes.len()
            )));
        }
        serde_bytes::serialize(&bytes, serializer)
    }
}

impl<'de> Deserialize<'de> for BitField {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let bytes: Cow<'de, [u8]> = serde_bytes::deserialize(deserializer)?;
        Self::from_bytes(&bytes).map_err(serde::de::Error::custom)
    }
}

impl BitField {
    /// Decodes RLE+ encoded bytes into a bit field.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let mut reader = BitReader::new(bytes);

        let version = reader.read(2);
        if version != 0 {
            return Err("incorrect version");
        }

        let mut next_value = reader.read(1) == 1;
        let mut ranges = Vec::new();
        let mut index = 0;

        while let Some(len) = reader.read_len()? {
            let start = index;
            index += len;
            let end = index;

            if next_value {
                ranges.push(start..end);
            }

            next_value = !next_value;
        }

        Ok(Self {
            ranges,
            ..Default::default()
        })
    }

    /// Turns a bit field into its RLE+ encoded form.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut iter = self.ranges();

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

        writer.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        super::{bitfield, ranges_from_bits},
        BitField, BitWriter,
    };

    use rand::{Rng, SeedableRng};
    use rand_xorshift::XorShiftRng;

    #[test]
    fn test() {
        for (bits, expected) in vec![
            (vec![], bitfield![]),
            (
                vec![
                    0, 0, // version
                    1, // starts with 1
                    0, 1, // fits into 4 bits
                    0, 0, 0, 1, // 8 - 1
                ],
                bitfield![1, 1, 1, 1, 1, 1, 1, 1],
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
                bitfield![1, 1, 1, 1, 0, 1, 1, 1],
            ),
            (
                vec![
                    0, 0, // version
                    1, // starts with 1
                    0, 0, // does not fit into 4 bits
                    1, 0, 0, 1, 1, 0, 0, 0, // 25 - 1
                ],
                bitfield![
                    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1
                ],
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
                bitfield![1],
            ),
        ] {
            let mut writer = BitWriter::new();
            for bit in bits {
                writer.write(bit, 1);
            }
            let bf = BitField::from_bytes(&writer.finish()).unwrap();
            assert_eq!(bf, expected);
        }
    }

    #[test]
    fn roundtrip() {
        let mut rng = XorShiftRng::seed_from_u64(1);

        for _i in 0..1000 {
            let len: usize = rng.gen_range(0, 1000);
            let bits: Vec<_> = (0..len).filter(|_| rng.gen::<bool>()).collect();

            let ranges: Vec<_> = ranges_from_bits(bits.clone()).collect();
            let bf = BitField::from_ranges(ranges_from_bits(bits));

            assert_eq!(bf.ranges().collect::<Vec<_>>(), ranges);
        }
    }
}
