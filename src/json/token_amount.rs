// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod json {
    use std::str::FromStr;

    use crate::shim::econ::TokenAmount;
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
}

#[cfg(test)]
mod tests {
    use crate::shim::econ::TokenAmount;
    use quickcheck_macros::quickcheck;

    use super::*;

    #[quickcheck]
    fn bigint_roundtrip(n: u64) {
        let token_amount = TokenAmount::from_atto(n);
        let serialized: String = crate::to_string_with!(&token_amount, json::serialize);
        let parsed = crate::from_str_with!(&serialized, json::deserialize);
        assert_eq!(token_amount, parsed);
    }
}
