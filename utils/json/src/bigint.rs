// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[cfg(test)]
use fvm_shared::bigint::BigInt;

pub mod json {
    use num_bigint::BigInt;
    use serde::{Deserialize, Serialize};
    use std::str::FromStr;

    /// Serializes `BigInt` as String
    pub fn serialize<S>(int: &BigInt, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        String::serialize(&int.to_string(), serializer)
    }

    /// De-serializes String into `BigInt`.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<BigInt, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        BigInt::from_str(&s).map_err(serde::de::Error::custom)
    }

    pub mod option {
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
    use super::*;
    use quickcheck_macros::quickcheck;

    macro_rules! to_string_with {
        ($obj:expr, $serializer:path) => {{
            let mut writer = Vec::new();
            $serializer($obj, &mut serde_json::ser::Serializer::new(&mut writer)).unwrap();
            String::from_utf8(writer).unwrap()
        }};
    }

    macro_rules! from_str_with {
        ($str:expr, $deserializer:path) => {
            $deserializer(&mut serde_json::de::Deserializer::from_str($str)).unwrap()
        };
    }

    #[quickcheck]
    fn bigint_roundtrip(bigint: BigInt) {
        let serialized: String = to_string_with!(&bigint, json::serialize);
        let parsed = from_str_with!(&serialized, json::deserialize);
        assert_eq!(bigint, parsed);
    }
}
