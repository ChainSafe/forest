// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::signature::BLS_SIG_LEN;
use encoding::{blake2b_256, serde_bytes};
use serde::{Deserialize, Serialize};

/// The output from running a VRF
#[derive(Clone, Debug, PartialEq, Eq, Ord, PartialOrd, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct VRFProof(#[serde(with = "serde_bytes")] pub Vec<u8>);

impl VRFProof {
    /// Creates a VRFProof from a raw vector
    pub fn new(output: Vec<u8>) -> Self {
        Self(output)
    }

    /// Returns reference to underlying vector
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Compute the blake2b256 digest of the proof
    pub fn digest(&self) -> [u8; 32] {
        blake2b_256(&self.0)
    }

    /// Returns max value based on [BLS_SIG_LEN](constant.BLS_SIG_LEN.html)
    pub fn max_value() -> Self {
        // TODO revisit if this is necessary
        Self::new([std::u8::MAX; BLS_SIG_LEN].to_vec())
    }
}

#[cfg(feature = "json")]
pub mod json {
    use super::*;
    use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

    // Wrapper for serializing and deserializing a VRFProof from JSON.
    #[derive(Deserialize, Serialize)]
    #[serde(transparent)]
    pub struct VRFProofJson(#[serde(with = "self")] pub VRFProof);

    /// Wrapper for serializing a VRFProof reference to JSON.
    #[derive(Serialize)]
    #[serde(transparent)]
    pub struct VRFProofJsonRef<'a>(#[serde(with = "self")] pub &'a VRFProof);

    #[derive(Serialize, Deserialize)]
    struct JsonHelper {
        #[serde(rename = "VRFProof")]
        bytes: String,
    }

    pub fn serialize<S>(m: &VRFProof, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        JsonHelper {
            bytes: base64::encode(&m.as_bytes()),
        }
        .serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<VRFProof, D::Error>
    where
        D: Deserializer<'de>,
    {
        let JsonHelper { bytes } = Deserialize::deserialize(deserializer)?;
        Ok(VRFProof::new(
            base64::decode(bytes).map_err(de::Error::custom)?,
        ))
    }

    pub mod opt {
        use super::{VRFProof, VRFProofJson, VRFProofJsonRef};
        use serde::{self, Deserialize, Deserializer, Serialize, Serializer};

        pub fn serialize<S>(v: &Option<VRFProof>, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            v.as_ref().map(|s| VRFProofJsonRef(s)).serialize(serializer)
        }

        pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<VRFProof>, D::Error>
        where
            D: Deserializer<'de>,
        {
            let s: Option<VRFProofJson> = Deserialize::deserialize(deserializer)?;
            Ok(s.map(|v| v.0))
        }
    }
}
