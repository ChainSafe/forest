// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use encoding::{de, ser, serde_bytes};
use std::{cmp, fmt, u8};

/// Map of bits to indicate which indexes contain values and which are empty
#[derive(PartialEq, Eq, Clone, Debug, Default, Copy)]
pub struct BitMap {
    b: u8,
}

impl ser::Serialize for BitMap {
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        let bitmap_bz = self.to_byte_array();
        let value = serde_bytes::Bytes::new(&bitmap_bz);
        serde_bytes::Serialize::serialize(value, s)
    }
}

impl<'de> de::Deserialize<'de> for BitMap {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let bz: Vec<u8> = serde_bytes::Deserialize::deserialize(deserializer)?;

        // Get bitmap byte from serialized bytes
        let bmap: BitMap = bz
            .get(0)
            .map(|b| BitMap::new(*b))
            .ok_or_else(|| de::Error::custom("Expected bitmap byte"))?;

        Ok(bmap)
    }
}

impl cmp::PartialEq<u8> for BitMap {
    fn eq(&self, other: &u8) -> bool {
        self.b == *other
    }
}

impl fmt::Binary for BitMap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:08b}", self.b)
    }
}

impl fmt::Display for BitMap {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:b}", self)
    }
}

impl BitMap {
    /// Constructor with predefined map
    pub fn new(b: u8) -> Self {
        Self { b }
    }

    /// Converts bitmap to array of bytes
    pub fn to_byte_array(self) -> [u8; 1] {
        [self.b]
    }

    /// Checks if bitmap is empty
    pub fn is_empty(self) -> bool {
        self.b == 0
    }

    /// Get bit from bitmap by index
    pub fn get_bit(self, i: u64) -> bool {
        self.b & (1 << i) != 0
    }

    /// Set bit in bitmap for index
    pub fn set_bit(&mut self, i: u64) {
        self.b |= 1 << i;
    }

    /// Clear bit at index for bitmap
    pub fn clear_bit(&mut self, i: u64) {
        self.b &= u8::MAX - (1 << i)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitmap() {
        let mut bmap = BitMap::new(0);
        assert_eq!(bmap.b, 0);
        bmap.set_bit(1);
        assert_eq!(bmap.get_bit(1), true);
        assert_eq!(bmap.b, 0b10);
        bmap.clear_bit(1);
        bmap.set_bit(0);
        assert_eq!(bmap.get_bit(0), true);
        assert_eq!(bmap.b, 0b1);
        bmap.set_bit(7);
        assert_eq!(bmap.get_bit(7), true);
        assert_eq!(bmap.b, 0b10000001);
    }
}
