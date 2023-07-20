#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BlockPosition {
    zst_frame_offset: u64,
    decoded_offset: u16,
}

impl BlockPosition {
    // Returns None if the two offets cannot be stored in a single u64
    pub fn new(zst_frame_offset: u64, decoded_offset: u16) -> Option<Self> {
        let position = BlockPosition {
            zst_frame_offset,
            decoded_offset,
        };
        if position.encode() == u64::MAX || BlockPosition::decode(position.encode()) != position {
            None
        } else {
            Some(position)
        }
    }

    pub fn try_from_le_bytes(bytes: [u8; 8]) -> Option<BlockPosition> {
        let n = u64::from_le_bytes(bytes);
        if n == u64::MAX {
            None
        } else{
            Some(Self::decode(u64::from_le_bytes(bytes)))
        }
    }

    pub fn to_le_bytes(self) -> [u8; 8] {
        self.encode().to_le_bytes()
    }

    fn encode(self) -> u64 {
        assert!(self.zst_frame_offset >> (u64::BITS - u16::BITS) == 0);
        self.zst_frame_offset << u16::BITS | self.decoded_offset as u64
    }

    fn decode(value: u64) -> Self {
        BlockPosition {
            zst_frame_offset: value >> u16::BITS,
            decoded_offset: value as u16,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::cid::CidCborExt;
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
