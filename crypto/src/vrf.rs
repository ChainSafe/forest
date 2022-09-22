// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::signature::verify_bls_sig;
use forest_encoding::{blake2b_256, serde_byte_array};
use fvm_shared::address::Address;
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

#[cfg(test)]
impl quickcheck::Arbitrary for VRFProof {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        let fmt_str = format!("===={}=====", u64::arbitrary(g));
        VRFProof::new(fmt_str.into_bytes())
    }
}

/// Verifies raw VRF proof. This VRF proof is a BLS signature.
pub fn verify_vrf(worker: &Address, vrf_base: &[u8], vrf_proof: &[u8]) -> Result<(), String> {
    verify_bls_sig(vrf_proof, vrf_base, worker).map_err(|e| format!("VRF was invalid: {}", e))
}

pub mod json {
    use super::*;
    use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
    use std::borrow::Cow;

    pub fn serialize<S>(m: &VRFProof, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        base64::encode(m.as_bytes()).serialize(serializer)
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

#[cfg(test)]
mod tests {
    use super::*;
    use quickcheck_macros::quickcheck;

    #[quickcheck]
    fn vrfproof_roundtrip(proof: VRFProof) {
        let serialized = serde_json::to_string(&proof).unwrap();
        println!("serialized {}", serialized);
        let parsed = serde_json::from_str(&serialized).unwrap();
        assert_eq!(proof, parsed);
    }
}
