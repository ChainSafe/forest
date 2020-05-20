// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use encoding::serde_bytes::{self, ByteBuf};
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use std::fmt::{self, Debug, Formatter};

/// String of random bytes
#[derive(PartialEq, Eq, Default, Copy, Clone)]
pub struct Randomness(pub [u8; 32]);

impl Debug for Randomness {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl Serialize for Randomness {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serde_bytes::serialize(&self.0[..], serializer)
    }
}

impl<'de> Deserialize<'de> for Randomness {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let bz_buf: ByteBuf = Deserialize::deserialize(deserializer)?;
        if bz_buf.len() != 32 {
            return Err(de::Error::custom("Array of bytes not length 32"));
        }
        let mut array = [0; 32];
        array.copy_from_slice(bz_buf.as_ref());
        Ok(Self(array))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use encoding::{from_slice, to_vec, BytesSer};

    #[test]
    fn array_symmetric_serialization() {
        let vec: Vec<u8> = (0..32).collect();
        let slice_bz = to_vec(&BytesSer(&vec)).unwrap();
        let Randomness(arr) = from_slice(&slice_bz).unwrap();
        // Check decoded array against slice
        assert_eq!(arr.as_ref(), vec.as_slice());
        // Check re-encoded array is equal to the slice encoded
        assert_eq!(to_vec(&BytesSer(&arr)).unwrap(), slice_bz);
    }
}
