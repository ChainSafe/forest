// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use super::{FrameOffset, Hash, KeyValuePair};

use std::io::{self, Read};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
pub enum Bucket {
    Empty,
    Full(KeyValuePair),
}

impl Bucket {
    pub const SIZE: usize = 16;

    pub fn to_le_bytes(self) -> [u8; Self::SIZE] {
        let (key, value) = match self {
            Bucket::Empty => (u64::MAX, u64::MAX.to_le_bytes()),
            Bucket::Full(entry) => (entry.hash.into(), entry.value.to_le_bytes()),
        };
        let mut output: [u8; 16] = [0; 16];
        output[0..8].copy_from_slice(&key.to_le_bytes());
        output[8..16].copy_from_slice(&value);
        output
    }

    pub fn from_le_bytes(bytes: [u8; Self::SIZE]) -> Self {
        let hash = Hash::from_le_bytes(bytes[0..8].try_into().expect("infallible"));
        if hash == Hash::INVALID {
            Bucket::Empty
        } else {
            let value = FrameOffset::from_le_bytes(bytes[8..16].try_into().expect("infallible"));
            Bucket::Full(KeyValuePair { hash, value })
        }
    }

    pub fn read(mut reader: impl Read) -> io::Result<Bucket> {
        let mut buffer = [0; Self::SIZE];
        reader.read_exact(&mut buffer)?;
        Ok(Bucket::from_le_bytes(buffer))
    }

    pub fn read_with_hash(mut reader: impl Read, hash: Hash) -> io::Result<Option<FrameOffset>> {
        let mut buffer = [0; Self::SIZE];
        reader.read_exact(&mut buffer)?;
        let disk_hash = Hash::from_le_bytes(buffer[0..8].try_into().expect("infallible"));
        if disk_hash == hash {
            Ok(Some(FrameOffset::from_le_bytes(
                buffer[8..16].try_into().expect("infallible"),
            )))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quickcheck_macros::quickcheck;

    #[quickcheck]
    fn forest_footer_roundtrip(slot: Bucket) {
        assert_eq!(slot, Bucket::from_le_bytes(slot.to_le_bytes()));
    }
}
