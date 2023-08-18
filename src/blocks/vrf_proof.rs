// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::utils::encoding::{blake2b_256, serde_byte_array};
use serde::{Deserialize, Serialize};

/// The output from running a VRF proof.
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
#[derive(Clone, Debug, PartialEq, Eq, Ord, PartialOrd, Default, Serialize, Deserialize)]
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
