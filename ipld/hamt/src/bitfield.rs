// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use byteorder::{BigEndian, ByteOrder};
use forest_encoding::{
    de::{Deserialize, Deserializer},
    ser::{Serialize, Serializer},
    serde_bytes,
};
use std::u64;

#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub struct Bitfield([u64; 4]);

impl Serialize for Bitfield {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut v = [0u8; 4 * 8];
        // Big endian ordering, to match go
        BigEndian::write_u64(&mut v[..8], self.0[3]);
        BigEndian::write_u64(&mut v[8..16], self.0[2]);
        BigEndian::write_u64(&mut v[16..24], self.0[1]);
        BigEndian::write_u64(&mut v[24..], self.0[0]);

        for i in 0..v.len() {
            if v[i] != 0 {
                return serde_bytes::Serialize::serialize(&v[i..], serializer);
            }
        }

        <[u8] as serde_bytes::Serialize>::serialize(&[], serializer)
    }
}

impl<'de> Deserialize<'de> for Bitfield {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let mut res = Bitfield::zero();
        let bytes = serde_bytes::ByteBuf::deserialize(deserializer)?.into_vec();

        let mut arr = [0u8; 4 * 8];
        let len = bytes.len();
        for (old, new) in bytes.iter().zip(arr[(32 - len)..].iter_mut()) {
            *new = *old;
        }
        res.0[3] = BigEndian::read_u64(&arr[..8]);
        res.0[2] = BigEndian::read_u64(&arr[8..16]);
        res.0[1] = BigEndian::read_u64(&arr[16..24]);
        res.0[0] = BigEndian::read_u64(&arr[24..]);

        Ok(res)
    }
}

impl Default for Bitfield {
    fn default() -> Self {
        Bitfield::zero()
    }
}

impl Bitfield {
    pub fn clear_bit(&mut self, idx: u32) {
        let ai = idx / 64;
        let bi = idx % 64;
        self.0[ai as usize] &= u64::MAX - (1 << bi as u32);
    }

    pub fn test_bit(&self, idx: u32) -> bool {
        let ai = idx / 64;
        let bi = idx % 64;

        self.0[ai as usize] & (1 << bi as u32) != 0
    }

    pub fn set_bit(&mut self, idx: u32) {
        let ai = idx / 64;
        let bi = idx % 64;

        self.0[ai as usize] |= 1 << bi as u32;
    }

    pub fn count_ones(&self) -> usize {
        self.0.iter().map(|a| a.count_ones() as usize).sum()
    }

    pub fn and(self, other: &Self) -> Self {
        Bitfield([
            self.0[0] & other.0[0],
            self.0[1] & other.0[1],
            self.0[2] & other.0[2],
            self.0[3] & other.0[3],
        ])
    }

    pub fn zero() -> Self {
        Bitfield([0, 0, 0, 0])
    }

    pub fn set_bits_le(self, bit: u32) -> Self {
        if bit == 0 {
            return self;
        }
        self.set_bits_leq(bit - 1)
    }

    pub fn set_bits_leq(mut self, bit: u32) -> Self {
        if bit < 64 {
            self.0[0] = set_bits_leq(self.0[0], bit);
        } else if bit < 128 {
            self.0[0] = std::u64::MAX;
            self.0[1] = set_bits_leq(self.0[1], bit - 64);
        } else if bit < 192 {
            self.0[0] = std::u64::MAX;
            self.0[1] = std::u64::MAX;
            self.0[2] = set_bits_leq(self.0[2], bit - 128);
        } else {
            self.0[0] = std::u64::MAX;
            self.0[1] = std::u64::MAX;
            self.0[2] = std::u64::MAX;
            self.0[3] = set_bits_leq(self.0[3], bit - 192);
        }

        self
    }
}

#[inline]
fn set_bits_leq(v: u64, bit: u32) -> u64 {
    (v as u128 | ((1u128 << (1 + bit)) - 1)) as u64
}

impl std::fmt::Binary for Bitfield {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let val = self.0;

        write!(f, "{:b}_{:b}_{:b}_{:b}", val[0], val[1], val[2], val[3])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forest_encoding::{from_slice, to_vec};

    #[test]
    fn test_bitfield() {
        let mut b = Bitfield::zero();
        b.set_bit(8);
        b.set_bit(18);
        b.set_bit(92);
        b.set_bit(255);
        assert!(b.test_bit(8));
        assert!(b.test_bit(18));
        assert!(!b.test_bit(19));
        assert!(b.test_bit(92));
        assert!(!b.test_bit(95));
        assert!(b.test_bit(255));

        b.clear_bit(18);
        assert!(!b.test_bit(18));
    }

    #[test]
    fn test_cbor_serialization() {
        let mut b0 = Bitfield::zero();
        let bz = to_vec(&b0).unwrap();
        assert_eq!(&bz, &[64]);
        assert_eq!(&from_slice::<Bitfield>(&bz).unwrap(), &b0);

        b0.set_bit(0);
        let bz = to_vec(&b0).unwrap();
        assert_eq!(&bz, &[65, 1]);
        assert_eq!(&from_slice::<Bitfield>(&bz).unwrap(), &b0);

        b0.set_bit(64);
        let bz = to_vec(&b0).unwrap();
        assert_eq!(&bz, &[73, 1, 0, 0, 0, 0, 0, 0, 0, 1]);
        assert_eq!(&from_slice::<Bitfield>(&bz).unwrap(), &b0);
    }
}
