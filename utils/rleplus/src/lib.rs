// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! # RLE+ Bitset Encoding
//!
//! RLE+ is a lossless compression format based on [RLE](https://en.wikipedia.org/wiki/Run-length_encoding).
//! It's primary goal is to reduce the size in the case of many individual bits, where RLE breaks down quickly,
//! while keeping the same level of compression for large sets of contigous bits.
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

pub mod bitvec_serde;

pub use bitvec;
use bitvec::prelude::{BitVec, Lsb0};

/// Encode the given bitset into their RLE+ encoded representation.
pub fn encode(raw: &BitVec<Lsb0, u8>) -> BitVec<Lsb0, u8> {
    let mut encoding = BitVec::new();

    if raw.is_empty() {
        return encoding;
    }

    // Header
    // encode version "00" and push to start of encoding
    encoding.insert(0, false);
    encoding.insert(0, false);

    // encode the very first bit (the first block contains this, then alternating)
    encoding.push(*raw.get(0).unwrap());

    // the running length
    let mut count = 1;

    // the current bit type
    let mut current = raw.get(0);

    let last = raw.len();

    for i in 1..=raw.len() {
        if raw.get(i) != current || i == last {
            if i == last && raw.get(i) == current {
                count += 1;
            }

            if count == 1 {
                // Block Single
                encoding.push(true);
            } else if count < 16 {
                // Block Short
                // 4 bits
                let s_vec: BitVec<Lsb0, u8> = BitVec::from(&[count as u8][..]);

                // prefix: 01
                encoding.push(false);
                encoding.push(true);
                encoding.extend(s_vec.into_iter().take(4));
                count = 1;
            } else {
                // Block Long
                let mut v = [0u8; 10];
                let s = unsigned_varint::encode::u64(count, &mut v);
                let s_vec: BitVec<Lsb0, u8> = BitVec::from(s);

                // prefix: 00
                encoding.push(false);
                encoding.push(false);

                encoding.extend(s_vec.into_iter());
                count = 1;
            }
            current = raw.get(i);
        } else {
            count += 1;
        }
    }

    encoding
}

/// Decode an RLE+ encoded bitset into its original form.
pub fn decode(enc: &BitVec<Lsb0, u8>) -> Result<BitVec<Lsb0, u8>, &'static str> {
    let mut decoded = BitVec::new();

    if enc.is_empty() {
        return Ok(decoded);
    }

    // Header
    if enc.len() < 3 {
        return Err("Failed to decode, bytes must be at least 3 bits long");
    }

    // read version (expects "00")
    if *enc.get(0).unwrap() || *enc.get(1).unwrap() {
        return Err("Invalid version, expected '00'");
    }

    // read the inital bit
    let mut cur = *enc.get(2).unwrap();

    // pointer into the encoded bitvec
    let mut i = 3;

    let len = enc.len();

    while i < len {
        // read the next prefix
        match enc.get(i).unwrap() {
            false => {
                // multiple bits
                match enc.get(i + 1) {
                    Some(false) => {
                        // Block Long
                        // prefix: 00

                        let buf = enc
                            .iter()
                            .skip(i + 2)
                            .take(10 * 8)
                            .copied()
                            .collect::<BitVec<Lsb0, u8>>();
                        let buf_ref: &[u8] = buf.as_ref();
                        let (len, rest) = unsigned_varint::decode::u64(buf_ref)
                            .map_err(|_| "Failed to decode uvarint")?;

                        // insert this many bits
                        decoded.extend((0..len).map(|_| cur));

                        // prefix
                        i += 2;
                        // this is how much space the varint took in bits
                        i += (buf_ref.len() * 8) - (rest.len() * 8);
                    }
                    Some(true) => {
                        // Block Short
                        // prefix: 01
                        let buf = enc
                            .iter()
                            .skip(i + 2)
                            .take(4)
                            .copied()
                            .collect::<BitVec<Lsb0, u8>>();
                        let res: Vec<u8> = buf.into();

                        if res.len() != 1 {
                            return Err("Invalid short block encoding");
                        }

                        let len = res[0] as usize;

                        // prefix
                        i += 2;
                        // length of the encoded number
                        i += 4;

                        decoded.extend((0..len).map(|_| cur));
                    }
                    None => {
                        return Err("premature end to bits");
                    }
                }
            }
            true => {
                // Block Signle
                decoded.push(cur);
                i += 1;
            }
        }

        // swith the cur value
        cur = !cur;
    }

    Ok(decoded)
}

#[cfg(test)]
mod tests {
    use super::*;

    use bitvec::*;
    use rand::{Rng, RngCore, SeedableRng};
    use rand_xorshift::XorShiftRng;

    #[test]
    fn test_rle_plus_basics() {
        let cases: Vec<(BitVec<Lsb0, u8>, BitVec<Lsb0, u8>)> = vec![
            (
                bitvec![Lsb0, u8; 0; 8],
                bitvec![Lsb0, u8;
                        0, 0, // version
                        0, // starts with 0
                        0, 1, // fits into 4 bits
                        0, 0, 0, 1, // 8
                ],
            ),
            (
                bitvec![Lsb0, u8; 0, 0, 0, 0, 1, 0, 0, 0],
                bitvec![Lsb0, u8;
                        0, 0, // version
                        0, // starts with 0
                        0, 1, // fits into 4 bits
                        0, 0, 1, 0, // 4 - 0
                        1, // 1 - 1
                        0, 1, // fits into 4 bits
                        1, 1, 0, 0 // 3 - 0
                ],
            ),
        ];

        for (i, case) in cases.into_iter().enumerate() {
            assert_eq!(encode(&case.0), case.1, "case: {}", i);
        }
    }

    #[test]
    #[ignore]
    fn test_rle_plus_roundtrip_small() {
        let mut rng = XorShiftRng::from_seed([1u8; 16]);

        for _i in 0..10000 {
            let len: usize = rng.gen_range(0, 1000);

            let mut src = vec![0u8; len];
            rng.fill_bytes(&mut src);

            let original: BitVec<Lsb0, u8> = src.into();

            let encoded = encode(&original);
            let decoded = decode(&encoded).unwrap();

            assert_eq!(original, decoded);
        }
    }

    #[test]
    #[ignore]
    fn test_rle_plus_roundtrip_large() {
        let mut rng = XorShiftRng::from_seed([2u8; 16]);

        for _i in 0..100 {
            let len: usize = rng.gen_range(0, 100000);

            let mut src = vec![0u8; len];
            rng.fill_bytes(&mut src);

            let original: BitVec<Lsb0, u8> = src.into();

            let encoded = encode(&original);
            let decoded = decode(&encoded).unwrap();

            assert_eq!(original, decoded);
        }
    }
}
