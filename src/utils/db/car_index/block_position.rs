// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
#![allow(dead_code)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BlockPosition(u64);

impl BlockPosition {
    // Returns None if the two offets cannot be stored in a single u64
    pub fn new(zst_frame_offset: u64) -> Self {
        BlockPosition(zst_frame_offset)
    }

    pub fn zst_frame_offset(self) -> u64 {
        self.0
    }

    pub fn from_le_bytes(bytes: [u8; 8]) -> BlockPosition {
        Self::decode(u64::from_le_bytes(bytes))
    }

    pub fn to_le_bytes(self) -> [u8; 8] {
        self.encode().to_le_bytes()
    }

    fn encode(self) -> u64 {
        self.0
    }

    pub fn decode(value: u64) -> Self {
        BlockPosition(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quickcheck::{Arbitrary, Gen};

    impl Arbitrary for BlockPosition {
        fn arbitrary(g: &mut Gen) -> BlockPosition {
            BlockPosition::new(u64::arbitrary(g))
        }
    }
}
