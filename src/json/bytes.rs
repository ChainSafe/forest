// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod json {
    use base64::{prelude::BASE64_STANDARD, Engine};
    use serde::{Deserialize, Serialize};

    /// Serializes `Vec<u8>` as Option<String>.
    pub fn serialize<S>(bytes: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let base64 = if bytes.is_empty() {
            None
        } else {
            Some(BASE64_STANDARD.encode(bytes))
        };
        <Option<String>>::serialize(&base64, serializer)
    }

    /// De-serializes Option<String> into `Vec<u8>`.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let base64 = <Option<String>>::deserialize(deserializer)?;
        match base64 {
            Some(s) => BASE64_STANDARD.decode(s).map_err(serde::de::Error::custom),
            None => Ok(vec![]),
        }
    }
}
