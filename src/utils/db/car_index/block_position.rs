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

    pub fn encode(self) -> u64 {
        assert!(self.zst_frame_offset >> (u64::BITS - u16::BITS) == 0);
        self.zst_frame_offset << u16::BITS | self.decoded_offset as u64
    }

    pub fn decode(value: u64) -> Self {
        BlockPosition {
            zst_frame_offset: value >> u16::BITS,
            decoded_offset: value as u16,
        }
    }
}
