// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::MAX_ENCODED_SIZE;
use num_bigint::{BigInt, Sign};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

/// Wrapper for serializing big ints to match filecoin spec. Serializes as bytes.
#[derive(Serialize)]
#[serde(transparent)]
pub struct BigIntSer<'a>(#[serde(with = "self")] pub &'a BigInt);

/// Wrapper for deserializing as BigInt from bytes.
#[derive(Deserialize, Serialize, Clone, Default, PartialEq)]
#[serde(transparent)]
pub struct BigIntDe(#[serde(with = "self")] pub BigInt);

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
    if bz.len() > MAX_ENCODED_SIZE {
        return Err(serde::ser::Error::custom(format!(
            "encoded big int was too large ({} bytes)",
            bz.len()
        )));
    }

    // Serialize as bytes
    serde_bytes::Serialize::serialize(&bz, serializer)
}

/// Deserializes bytes into big int.
pub fn deserialize<'de, D>(deserializer: D) -> Result<BigInt, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let bz: Cow<'de, [u8]> = serde_bytes::Deserialize::deserialize(deserializer)?;
    if bz.is_empty() {
        return Ok(BigInt::default());
    }
    if bz.len() > MAX_ENCODED_SIZE {
        return Err(serde::de::Error::custom(format!(
            "decoded big int was too large ({} bytes)",
            bz.len()
        )));
    }
    let sign_byte = bz[0];
    let sign: Sign = match sign_byte {
        1 => Sign::Minus,
        0 => Sign::Plus,
        _ => {
            return Err(serde::de::Error::custom(
                "First byte must be valid sign (0, 1)",
            ));
        }
    };
    Ok(BigInt::from_bytes_be(sign, &bz[1..]))
}

#[cfg(feature = "json")]
pub mod json {
    use super::*;
    use std::str::FromStr;

    /// Serializes BigInt as String
    pub fn serialize<S>(int: &BigInt, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        String::serialize(&int.to_string(), serializer)
    }

    /// Deserializes String into BigInt.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<BigInt, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        BigInt::from_str(&s).map_err(serde::de::Error::custom)
    }

    pub mod opt {
        use super::*;
        use serde::{self, Deserialize, Deserializer, Serialize, Serializer};

        pub fn serialize<S>(v: &Option<BigInt>, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            v.as_ref().map(|s| s.to_string()).serialize(serializer)
        }

        pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<BigInt>, D::Error>
        where
            D: Deserializer<'de>,
        {
            let s: Option<String> = Deserialize::deserialize(deserializer)?;
            if let Some(v) = s {
                return Ok(Some(
                    BigInt::from_str(&v).map_err(serde::de::Error::custom)?,
                ));
            }
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::bigint_ser::{deserialize, serialize};
    use num_bigint::{BigInt, Sign};
    use serde_cbor::de::Deserializer;
    use serde_cbor::ser::Serializer;

    #[test]
    fn serialize_bigint_test() {
        // Create too large BigInt
        let mut digits: Vec<u32> = Vec::new();
        for _ in 0..32 {
            digits.push(u32::MAX);
        }
        let bi = BigInt::new(Sign::Plus, digits);

        // Serialize should fail
        let mut cbor = Vec::new();
        let res = serialize(&bi, &mut Serializer::new(&mut cbor));
        assert!(res.is_err());
    }

    #[test]
    fn deserialize_bigint_test() {
        // Create a 129 bytes large BigInt
        let mut bytes = vec![u8::MAX; 129];
        bytes[0] = 0;

        // Serialize manually
        let mut cbor = Vec::new();
        serde_bytes::serialize(&bytes, &mut Serializer::new(&mut cbor)).unwrap();

        // Deserialize should fail
        let res = deserialize(&mut Deserializer::from_slice(&cbor));
        assert!(res.is_err());

        // Create a 128 bytes BigInt
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
