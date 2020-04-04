// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use serde_bytes::ByteBuf;

/// Wrapper for serializing slice of bytes.
#[derive(Serialize)]
#[serde(transparent)]
pub struct BytesSer<'a>(#[serde(with = "serde_bytes")] pub &'a [u8]);

/// Wrapper for deserializing dynamic sized Bytes.
#[derive(Deserialize, Serialize)]
#[serde(transparent)]
pub struct BytesDe(#[serde(with = "serde_bytes")] pub Vec<u8>);

/// Wrapper for deserializing array of 32 Bytes.
pub struct Byte32De(pub [u8; 32]);

impl Serialize for Byte32De {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        <[u8] as serde_bytes::Serialize>::serialize(&self.0, serializer)
    }
}

impl<'de> Deserialize<'de> for Byte32De {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let bz_buf: ByteBuf = Deserialize::deserialize(deserializer)?;
        if bz_buf.len() != 32 {
            return Err(de::Error::custom("Array of bytes not length 32"));
        }
        let mut array = [0; 32];
        array.copy_from_slice(bz_buf.as_ref());
        Ok(Byte32De(array))
    }
}
