// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{decode_and_apply_cache, rleplus::encode, BitField};
use bitvec::prelude::BitVec;
use serde::{ser, Deserialize, Deserializer, Serialize, Serializer};

impl Serialize for BitField {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            BitField::Encoded { bv, set, unset } => {
                if set.is_empty() && unset.is_empty() {
                    serde_bytes::serialize(bv.as_slice(), serializer)
                } else {
                    let decoded =
                        decode_and_apply_cache(bv, set, unset).map_err(ser::Error::custom)?;
                    serde_bytes::serialize(encode(&decoded).as_slice(), serializer)
                }
            }
            BitField::Decoded(bv) => serde_bytes::serialize(encode(bv).as_slice(), serializer),
        }
    }
}

impl<'de> Deserialize<'de> for BitField {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let bz: Vec<u8> = serde_bytes::deserialize(deserializer)?;
        Ok(BitField::Encoded {
            bv: BitVec::from_vec(bz),
            set: Default::default(),
            unset: Default::default(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitvec::bitvec;
    use encoding::{from_slice, to_vec};

    #[test]
    fn serialize_node_symmetric() {
        let bit_field: BitField = bitvec![Lsb0, u8; 0, 1, 0, 1, 1, 1, 1, 1, 1].into();
        let cbor_bz = to_vec(&bit_field).unwrap();
        let mut deserialized: BitField = from_slice(&cbor_bz).unwrap();
        assert_eq!(deserialized.count().unwrap(), 7);
        // assert_eq!(deserialized, bit_field);
    }

    #[test]
    // ported test from specs-actors `bitfield_test.go` with added vector
    fn bit_vec_unset_vector() {
        let mut bf = BitField::default();
        bf.set(1);
        bf.set(2);
        bf.set(3);
        bf.set(4);
        bf.set(5);

        bf.unset(3);

        assert_ne!(bf.get(3).unwrap(), true);
        assert_eq!(bf.count().unwrap(), 4);

        // Test cbor marshal and unmarshal
        let cbor_bz = to_vec(&bf).unwrap();
        assert_eq!(&cbor_bz, &[0x43, 0xa8, 0x54, 0x0]);
        let mut deserialized: BitField = from_slice(&cbor_bz).unwrap();

        assert_eq!(deserialized.count().unwrap(), 4);
        assert_ne!(bf.get(3).unwrap(), true);
    }
}
