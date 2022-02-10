// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::BYTE_ARRAY_MAX_LEN;
use serde::{de, ser, Deserializer, Serializer};
use serde_bytes::{Deserialize, Serialize};

/// serde_bytes with max length check
pub mod serde_byte_array {
    use super::*;

    /// checked if output > `crate::BYTE_ARRAY_MAX_LEN`
    pub fn serialize<T, S>(bytes: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        T: ?Sized + Serialize + AsRef<[u8]>,
        S: Serializer,
    {
        let len = bytes.as_ref().len();
        if len > BYTE_ARRAY_MAX_LEN {
            return Err(ser::Error::custom::<String>(
                "Array exceed max length".into(),
            ));
        }

        Serialize::serialize(bytes, serializer)
    }

    /// checked if input > `crate::ByteArrayMaxLen`
    pub fn deserialize<'de, T, D>(deserializer: D) -> Result<T, D::Error>
    where
        T: Deserialize<'de> + AsRef<[u8]>,
        D: Deserializer<'de>,
    {
        Deserialize::deserialize(deserializer).and_then(|bytes: T| {
            if bytes.as_ref().len() > BYTE_ARRAY_MAX_LEN {
                Err(de::Error::custom::<String>(
                    "Array exceed max length".into(),
                ))
            } else {
                Ok(bytes)
            }
        })
    }
}
