// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[allow(unused)] // TODO(aatifsyed)
pub mod json {
    use std::str::FromStr;

    use num_bigint::BigInt;
    use serde::{Deserialize, Serialize};

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
}

#[cfg(test)]
mod tests {
    use num::BigInt;
    use quickcheck_macros::quickcheck;

    use super::*;

    #[quickcheck]
    fn bigint_roundtrip(bigint: BigInt) {
        let serialized: String = crate::to_string_with!(&bigint, json::serialize);
        let parsed = crate::from_str_with!(&serialized, json::deserialize);
        assert_eq!(bigint, parsed);
    }
}
