use super::BlockPosition;
use super::Hash;
use super::KeyValuePair;
use std::io::{Read, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Empty,
    Full(KeyValuePair),
}

impl Slot {
    pub const SIZE: usize = 16;

    pub fn to_le_bytes(self) -> [u8; Self::SIZE] {
        let (key, value) = match self {
            Slot::Empty => (u64::MAX, u64::MAX.to_le_bytes()),
            Slot::Full(entry) => (entry.hash.into(), entry.value.to_le_bytes()),
        };
        let mut output: [u8; 16] = [0; 16];
        output[0..8].copy_from_slice(&key.to_le_bytes());
        output[8..16].copy_from_slice(&value);
        output
    }

    pub fn from_le_bytes(bytes: [u8; Self::SIZE]) -> Self {
        let hash = Hash::from_le_bytes(bytes[0..8].try_into().expect("infallible"));
        let mb_blockposition =
            BlockPosition::try_from_le_bytes(bytes[8..16].try_into().expect("infallible"));
        if let Some(value) = mb_blockposition {
            Slot::Full(KeyValuePair { hash, value })
        } else {
            Slot::Empty
        }
    }

    pub fn read(reader: &mut impl Read) -> Result<Slot> {
        let mut buffer = [0; Self::SIZE];
        reader.read_exact(&mut buffer)?;
        Ok(Slot::from_le_bytes(buffer))
    }
}
