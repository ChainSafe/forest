// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use num_bigint::{BigInt, Sign};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Wrapper for serializing big ints to match filecoin spec. Serializes as bytes.
pub struct BigIntSer<'a>(pub &'a BigInt);

/// Wrapper for deserializing as BigInt from bytes.
pub struct BigIntDe(pub BigInt);

impl Serialize for BigIntSer<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serialize(self.0, serializer)
    }
}

impl<'de> Deserialize<'de> for BigIntDe {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(BigIntDe(deserialize(deserializer)?))
    }
}

/// Serializes big int as bytes following Filecoin spec.
pub fn serialize<S>(int: &BigInt, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let (sign, mut bz) = int.to_bytes_be();

    // Insert sign byte at start of encoded bytes
    match sign {
        Sign::Minus => bz.insert(0, 1),
        Sign::Plus => bz.insert(0, 0),
        Sign::NoSign => bz = Vec::new(),
    }

    // Serialize as bytes
    let value = serde_bytes::Bytes::new(&bz);
    value.serialize(serializer)
}

/// Deserializes bytes into big int.
pub fn deserialize<'de, D>(deserializer: D) -> Result<BigInt, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let mut bz: Vec<u8> = serde_bytes::Deserialize::deserialize(deserializer)?;
    if bz.is_empty() {
        return Ok(BigInt::default());
    }
    let sign_byte = bz.remove(0);
    let sign: Sign = match sign_byte {
        1 => Sign::Minus,
        0 => Sign::Plus,
        _ => {
            return Err(serde::de::Error::custom(
                "First byte must be valid sign (0, 1)",
            ));
        }
    };
    Ok(BigInt::from_bytes_be(sign, &bz))
}
