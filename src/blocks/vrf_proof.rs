// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::utils::encoding::serde_byte_array;
use get_size2::GetSize;
use serde::{Deserialize, Serialize};

/// The output from running a VRF proof.
#[cfg_attr(
    test,
    derive(derive_quickcheck_arbitrary::Arbitrary, derive_more::Constructor)
)]
#[derive(Clone, Debug, PartialEq, Eq, Ord, PartialOrd, Default, Serialize, Deserialize, Hash)]
pub struct VRFProof(#[serde(with = "serde_byte_array")] pub Vec<u8>);

impl VRFProof {
    /// Returns reference to underlying proof bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Compute the `BLAKE2b256` digest of the proof.
    #[allow(dead_code)]
    pub fn digest(&self) -> [u8; 32] {
        crate::utils::encoding::blake2b_256(&self.0)
    }
}

impl GetSize for VRFProof {
    fn get_heap_size(&self) -> usize {
        self.0.get_heap_size()
    }
}
