// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod json {
    use std::str::FromStr;

    use fvm_shared::econ::TokenAmount;
    use num_bigint::BigInt;
    use serde::{Deserialize, Serialize};

    /// Serializes `TokenAmount` as String
    pub fn serialize<S>(token_amount: &TokenAmount, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        String::serialize(&token_amount.atto().to_string(), serializer)
    }

    /// De-serializes String into `TokenAmount`.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<TokenAmount, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(TokenAmount::from_atto(
            BigInt::from_str(&s).map_err(serde::de::Error::custom)?,
        ))
    }

    pub mod option {
        use serde::{self, Deserialize, Deserializer, Serialize, Serializer};

        use super::*;

        pub fn serialize<S>(v: &Option<TokenAmount>, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            v.as_ref()
                .map(|s| s.atto().to_string())
                .serialize(serializer)
        }

        pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<TokenAmount>, D::Error>
        where
            D: Deserializer<'de>,
        {
            let s: Option<String> = Deserialize::deserialize(deserializer)?;
            if let Some(v) = s {
                return Ok(Some(TokenAmount::from_atto(
                    BigInt::from_str(&v).map_err(serde::de::Error::custom)?,
                )));
            }
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use fvm_shared::econ::TokenAmount;
    use quickcheck_macros::quickcheck;

    use super::*;

    #[quickcheck]
    fn bigint_roundtrip(n: u64) {
        let token_amount = TokenAmount::from_atto(n);
        let serialized: String = forest_test_utils::to_string_with!(&token_amount, json::serialize);
        let parsed = forest_test_utils::from_str_with!(&serialized, json::deserialize);
        assert_eq!(token_amount, parsed);
    }
}
