// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

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
    use fvm_shared::bigint::BigInt;
    use quickcheck_macros::quickcheck;

    #[quickcheck]
    fn bigint_roundtrip(bigint: BigInt) {
        let serialized: String = forest_test_utils::to_string_with!(&bigint, json::serialize);
        let parsed = forest_test_utils::from_str_with!(&serialized, json::deserialize);
        assert_eq!(bigint, parsed);
    }
}
