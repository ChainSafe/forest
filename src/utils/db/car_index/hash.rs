// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use cid::Cid;
use std::ops::Not;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Hash(pub u64);

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
        Hash(hash)
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

    // Optimal offset for a hash with a given table length
    pub fn optimal_offset(&self, len: usize) -> usize {
        self.0 as usize % len
    }

    // Walking distance between `at` and the optimal location of `hash`
    pub fn distance(&self, at: usize, len: usize) -> usize {
        let pos = self.optimal_offset(len);
        if pos > at {
            (len - pos + at) % len
        } else {
            (at - pos) % len
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
        assert!(hash.optimal_offset(len.into()) < len.into())
    }

    #[quickcheck]
    fn hash_roundtrip(hash: Hash) {
        assert_eq!(hash, Hash::from_le_bytes(hash.to_le_bytes()))
    }

    #[quickcheck]
    fn hash_distance_range(hash: Hash, at: usize, len: NonZeroUsize) {
        // A hash can never be more than len-1 steps away from its optimal offset
        assert!(hash.distance(at % usize::from(len), len.into()) < len.into())
    }

    #[test]
    fn key_value_pair_distance_1() {
        // Hash(0) is right where it wants to be
        assert_eq!(Hash(0).distance(0, 1), 0);
    }

    #[test]
    fn key_value_pair_distance_2() {
        // If Hash(0) is at position 4 then it is 4 places away from where it wants to be.
        assert_eq!(Hash(0).distance(4, 10), 4);
    }
    #[test]
    fn key_value_pair_distance_3() {
        assert_eq!(Hash(9).distance(9, 10), 0);
        assert_eq!(Hash(9).distance(0, 10), 1);
    }
}
