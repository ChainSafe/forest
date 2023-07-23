// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use cid::Cid;
use std::ops::Not;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Hash(u64);

impl Hash {
    pub const INVALID: Hash = Hash(u64::MAX);
}

impl Not for Hash {
    type Output = Hash;
    fn not(self) -> Hash {
        Hash(self.0.not())
    }
}

impl From<Hash> for u64 {
    fn from(Hash(hash): Hash) -> u64 {
        hash
    }
}

impl From<u64> for Hash {
    fn from(hash: u64) -> Hash {
        // Clear top bit. It is used to indicate empty slots.
        Hash(hash & (u64::MAX >> 1))
    }
}

impl From<Cid> for Hash {
    fn from(cid: Cid) -> Hash {
        Hash::from_le_bytes(cid.hash().digest()[0..8].try_into().unwrap_or([0xFF; 8]))
    }
}

impl Hash {
    pub fn from_le_bytes(bytes: [u8; 8]) -> Hash {
        Hash(u64::from_le_bytes(bytes))
    }

    pub fn to_le_bytes(self) -> [u8; 8] {
        self.0.to_le_bytes()
    }


    // See: https://lemire.me/blog/2016/06/27/a-fast-alternative-to-the-modulo-reduction/
    // Desired bucket for a hash with a given table length
    pub fn bucket(&self, len: u64) -> u64 {
        // self.0 as usize % len
        // break 0..=u64::MAX into 'len' chunks and map each chunk to 0..len.
        // if len=2, 0..(u64::MAX/2) maps to 0, and (u64::MAX/2)..=u64::MAX maps to 1.
        ((self.0 as u128 * len as u128) >> 64) as u64
    }

    // hash.set_offset(x, len).optimal_offset(len) = x
    pub fn set_offset(self, offset: u64, len: u64) -> Self {
        fn div_ceil(a: u128, b: u128) -> u64 {
            (a / b + (if a % b == 0 { 0 } else { 1 })) as u64
        }
        // min with offset
        let min_with_offset = div_ceil((1_u128 << u64::BITS) * offset as u128, len as u128);
        let offset_height = u64::MAX / len;
        Hash(min_with_offset + self.0 % offset_height)
    }

    // Walking distance between `at` and the optimal location of `hash`
    pub fn distance(&self, at: u64, len: u64) -> u64 {
        let pos = self.bucket(len);
        if pos > at {
            len - pos + at
        } else {
            at - pos
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quickcheck::{Arbitrary, Gen};
    use quickcheck_macros::quickcheck;
    use std::num::NonZeroUsize;

    impl Arbitrary for Hash {
        fn arbitrary(g: &mut Gen) -> Hash {
            Hash::from(u64::arbitrary(g))
        }
    }

    #[quickcheck]
    fn hash_offset_range(hash: Hash, len: NonZeroUsize) {
        // The optimal offset must be in 0..len
        assert!(hash.bucket(usize::from(len) as u64) < usize::from(len) as u64)
    }

    #[quickcheck]
    fn hash_roundtrip(hash: Hash) {
        assert_eq!(hash, Hash::from_le_bytes(hash.to_le_bytes()))
    }

    #[quickcheck]
    fn hash_set_offset(hash: Hash, mut offset: u64, mut len: u64) {
        len = len.saturating_add(1); // len is non-zero
        offset %= len; // offset is smaller than len
        assert_eq!(offset, hash.set_offset(offset, len).bucket(len))
    }

    // small offsets and lengths can be tested exhaustively
    #[quickcheck]
    fn hash_set_offset_small(hash: Hash) {
        for len in 1..u8::MAX {
            for offset in 0..len {
                assert_eq!(
                    offset as u64,
                    hash.set_offset(offset as u64, len as u64)
                        .bucket(len as u64),
                    "failed to set offset with len={len} and offset={offset}"
                )
            }
        }
    }

    #[quickcheck]
    fn hash_distance_range(hash: Hash, at: u64, len: NonZeroUsize) {
        // A hash can never be more than len-1 steps away from its optimal offset
        assert!(hash.distance(at % len.get() as u64, len.get() as u64) < len.get() as u64)
    }

    #[test]
    fn hash_distance_1() {
        // Hash(0) is right where it wants to be
        assert_eq!(Hash(0).distance(0, 1), 0);
    }

    #[test]
    fn hash_distance_2() {
        // If Hash(0) is at position 4 then it is 4 places away from where it wants to be.
        assert_eq!(Hash(0).distance(4, 10), 4);
    }
}
