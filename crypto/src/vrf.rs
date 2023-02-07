// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_encoding::{blake2b_256, serde_byte_array};
use serde::{Deserialize, Serialize};

/// The output from running a VRF proof.
#[derive(Clone, Debug, PartialEq, Eq, Ord, PartialOrd, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct VRFProof(#[serde(with = "serde_byte_array")] pub Vec<u8>);

impl VRFProof {
    /// Creates a `VRFProof` from a raw vector.
    pub fn new(output: Vec<u8>) -> Self {
        Self(output)
    }

    /// Returns reference to underlying proof bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Compute the `BLAKE2b256` digest of the proof.
    pub fn digest(&self) -> [u8; 32] {
        blake2b_256(&self.0)
    }
}

pub mod json {
    use std::borrow::Cow;

    use base64::{prelude::BASE64_STANDARD, Engine};
    use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

    use super::*;

    pub fn serialize<S>(m: &VRFProof, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        BASE64_STANDARD.encode(m.as_bytes()).serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<VRFProof, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: Cow<'de, str> = Deserialize::deserialize(deserializer)?;
        Ok(VRFProof::new(
            BASE64_STANDARD
                .decode(s.as_ref())
                .map_err(de::Error::custom)?,
        ))
    }
}
