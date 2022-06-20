// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::MAX_ENCODED_SIZE;
use num_bigint::BigUint;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

/// Wrapper for serializing big ints to match filecoin spec. Serializes as bytes.
#[derive(Serialize)]
#[serde(transparent)]
pub struct BigUintSer<'a>(#[serde(with = "self")] pub &'a BigUint);

/// Wrapper for deserializing as BigUint from bytes.
#[derive(Deserialize, Serialize, Clone)]
#[serde(transparent)]
pub struct BigUintDe(#[serde(with = "self")] pub BigUint);

/// Serializes big uint as bytes.
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
    if bz.len() > MAX_ENCODED_SIZE {
        return Err(serde::ser::Error::custom(format!(
            "encoded big int was too large ({} bytes)",
            bz.len()
        )));
    }

    // Serialize as bytes
    serde_bytes::Serialize::serialize(&bz, serializer)
}

/// Deserializes bytes into big uint.
pub fn deserialize<'de, D>(deserializer: D) -> Result<BigUint, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let bz: Cow<'de, [u8]> = serde_bytes::Deserialize::deserialize(deserializer)?;
    if bz.is_empty() {
        return Ok(BigUint::default());
    }
    if bz.len() > MAX_ENCODED_SIZE {
        return Err(serde::de::Error::custom(format!(
            "decoded big uint was too large ({} bytes)",
            bz.len()
        )));
    }

    if bz.first() != Some(&0) {
        return Err(serde::de::Error::custom(
            "First byte must be 0 to decode as BigUint",
        ));
    }

    Ok(BigUint::from_bytes_be(&bz[1..]))
}

#[cfg(test)]
mod tests {
    use crate::biguint_ser::{deserialize, serialize};
    use num_bigint::BigUint;
    use serde_ipld_dagcbor::{Deserializer, Serializer};

    #[test]
    fn serialize_biguint_test() {
        // Create too large BigUint
        let mut digits: Vec<u32> = Vec::new();
        for _ in 0..32 {
            digits.push(u32::MAX);
        }
        let bi = BigUint::new(digits);

        // Serialize should fail
        let mut cbor = Vec::new();
        let res = serialize(&bi, &mut Serializer::new(&mut cbor));
        assert!(res.is_err());
    }

    #[test]
    fn deserialize_biguint_test() {
        // Create a 129 bytes large BigUint
        let mut bytes = vec![u8::MAX; 129];
        bytes[0] = 0;

        // Serialize manually
        let mut cbor = Vec::new();
        serde_bytes::serialize(&bytes, &mut Serializer::new(&mut cbor)).unwrap();

        // Deserialize should fail
        let res = deserialize(&mut Deserializer::from_slice(&cbor));
        assert!(res.is_err());

        // Create a 128 bytes BigUint
        let mut bytes = vec![u8::MAX; 128];
        bytes[0] = 0;

        // Serialize manually
        let mut cbor = Vec::new();
        serde_bytes::serialize(&bytes, &mut Serializer::new(&mut cbor)).unwrap();

        // Deserialize should work
        let res = deserialize(&mut Deserializer::from_slice(&cbor));
        assert!(res.is_ok());
    }
}
