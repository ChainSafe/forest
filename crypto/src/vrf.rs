// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::signature::verify_bls_sig;
use address::Address;
use encoding::{blake2b_256, serde_bytes};
use serde::{Deserialize, Serialize};

/// The output from running a VRF proof.
#[derive(Clone, Debug, PartialEq, Eq, Ord, PartialOrd, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct VRFProof(#[serde(with = "serde_bytes")] pub Vec<u8>);

impl VRFProof {
    /// Creates a VRFProof from a raw vector.
    pub fn new(output: Vec<u8>) -> Self {
        Self(output)
    }

    /// Returns reference to underlying proof bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Compute the blake2b256 digest of the proof.
    pub fn digest(&self) -> [u8; 32] {
        blake2b_256(&self.0)
    }
}

/// Verifies raw VRF proof. This VRF proof is a BLS signature.
pub fn verify_vrf(worker: &Address, vrf_base: &[u8], vrf_proof: &[u8]) -> Result<(), String> {
    verify_bls_sig(vrf_proof, vrf_base, worker).map_err(|e| format!("VRF was invalid: {}", e))
}

#[cfg(feature = "json")]
pub mod json {
    use super::*;
    use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
    use std::borrow::Cow;

    pub fn serialize<S>(m: &VRFProof, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        base64::encode(&m.as_bytes()).serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<VRFProof, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: Cow<'de, str> = Deserialize::deserialize(deserializer)?;
        Ok(VRFProof::new(
            base64::decode(s.as_ref()).map_err(de::Error::custom)?,
        ))
    }
}
