// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
#![allow(dead_code)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BlockPosition(u64);

impl BlockPosition {
    // Returns None if the two offets cannot be stored in a single u64
    pub fn new(zst_frame_offset: u64, decoded_offset: u16) -> Option<Self> {
        if zst_frame_offset >> (64 - 16) != 0 {
            None
        } else {
            Some(BlockPosition(
                zst_frame_offset << 16 | decoded_offset as u64,
            ))
        }
    }

    pub fn zst_frame_offset(self) -> u64 {
        self.0 >> 16
    }
    pub fn decoded_offset(self) -> u16 {
        self.0 as u16
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
    use quickcheck_macros::quickcheck;

    impl Arbitrary for BlockPosition {
        fn arbitrary(g: &mut Gen) -> BlockPosition {
            BlockPosition::new(
                (u64::arbitrary(g) >> u16::BITS).saturating_sub(1),
                u16::arbitrary(g),
            )
            .unwrap()
        }
    }

    #[quickcheck]
    fn position_roundtrip(p: BlockPosition) {
        assert_eq!(p, BlockPosition::decode(p.encode()))
    }
}
