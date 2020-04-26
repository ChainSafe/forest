// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{decode, encode};
use bitvec::prelude::{BitVec, Lsb0};
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

/// Wrapper for serializing bit vector with RLE+ encoding
pub struct BitVecSer<'a>(pub &'a BitVec<Lsb0, u8>);

/// Wrapper for deserializing bit vector with RLE+ decoding from bytes.
pub struct BitVecDe(pub BitVec<Lsb0, u8>);

impl Serialize for BitVecSer<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // This serialize will encode into rle+ before serializing
        serde_bytes::serialize(encode(self.0).as_slice(), serializer)
    }
}

impl<'de> Deserialize<'de> for BitVecDe {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Deserialize will decode using rle+ decompression
        let bz: Vec<u8> = serde_bytes::deserialize(deserializer)?;
        let compressed = BitVec::from_vec(bz);
        Ok(BitVecDe(decode(&compressed).map_err(de::Error::custom)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitvec::bitvec;
    use encoding::{from_slice, to_vec};

    #[test]
    fn serialize_node_symmetric() {
        let bit_vec = bitvec![Lsb0, u8; 0, 1, 0, 1, 1, 1, 1, 1, 1];
        let cbor_bz = to_vec(&BitVecSer(&bit_vec)).unwrap();
        let BitVecDe(deserialized) = from_slice::<BitVecDe>(&cbor_bz).unwrap();
        assert_eq!(deserialized.count_ones(), 7);
        assert_eq!(deserialized.as_slice(), bit_vec.as_slice());
    }

    #[test]
    // ported test from specs-actors `bitfield_test.go` with added vector
    fn bit_vec_unset_vector() {
        let mut bv: BitVec<Lsb0, u8> = BitVec::with_capacity(6);
        bv.resize(6, false);
        bv.set(1, true);
        bv.set(2, true);
        bv.set(3, true);
        bv.set(4, true);
        bv.set(5, true);

        bv.set(3, false);
        assert_ne!(bv.get(3), Some(&true));
        assert_eq!(bv.count_ones(), 4);

        // Test cbor marshal and unmarshal
        let cbor_bz = to_vec(&BitVecSer(&bv)).unwrap();
        assert_eq!(&cbor_bz, &[0x43, 0xa8, 0x54, 0x0]);
        let BitVecDe(deserialized) = from_slice::<BitVecDe>(&cbor_bz).unwrap();

        assert_eq!(deserialized.count_ones(), 4);
        assert_ne!(bv.get(3), Some(&true));
    }
}
