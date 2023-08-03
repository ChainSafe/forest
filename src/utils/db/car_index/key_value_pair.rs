// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use super::Hash;

pub type FrameOffset = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
pub struct KeyValuePair {
    pub hash: Hash,
    pub value: FrameOffset,
}

impl KeyValuePair {
    // Optimal offset for a hash with a given table length
    pub fn bucket(&self, len: u64) -> u64 {
        self.hash.bucket(len)
    }

    // Walking distance between `at` and the optimal location of `hash`
    pub fn distance(&self, at: u64, len: u64) -> u64 {
        self.hash.distance(at, len)
    }
}
