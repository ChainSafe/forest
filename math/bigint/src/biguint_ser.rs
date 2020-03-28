// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use num_bigint::BigUint;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Wrapper for serializing big ints to match filecoin spec. Serializes as bytes.
pub struct BigUintSer<'a>(pub &'a BigUint);

/// Wrapper for deserializing as BigUint from bytes.
pub struct BigUintDe(pub BigUint);

impl Serialize for BigUintSer<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serialize(self.0, serializer)
    }
}

impl Serialize for BigUintDe {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serialize(&self.0, serializer)
    }
}

impl<'de> Deserialize<'de> for BigUintDe {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(BigUintDe(deserialize(deserializer)?))
    }
}

pub fn serialize<S>(int: &BigUint, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let mut bz = int.to_bytes_be();

    // Insert positive sign byte at start of encoded bytes if non-zero
    if bz == [0] {
        bz = Vec::new()
    } else {
        bz.insert(0, 0);
    }

    // Serialize as bytes
    let value = serde_bytes::Bytes::new(&bz);
    value.serialize(serializer)
}

pub fn deserialize<'de, D>(deserializer: D) -> Result<BigUint, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let mut bz: Vec<u8> = serde_bytes::Deserialize::deserialize(deserializer)?;
    if bz.is_empty() {
        return Ok(BigUint::default());
    }

    if bz.remove(0) != 0 {
        return Err(serde::de::Error::custom(
            "First byte must be 0 to decode as BigUint",
        ));
    }

    Ok(BigUint::from_bytes_be(&bz))
}
