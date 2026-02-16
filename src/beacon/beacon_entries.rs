// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::utils::encoding::serde_byte_array;
use byteorder::{BigEndian, ByteOrder as _};
use fvm_ipld_encoding::tuple::*;
use get_size2::GetSize;
use sha2::digest::Digest as _;

/// The result from getting an entry from `Drand`.
/// The entry contains the round, or epoch as well as the BLS signature for that
/// round of randomness.
/// This beacon entry is stored on chain in the block header.
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
#[derive(
    Clone,
    Debug,
    Default,
    Eq,
    PartialEq,
    Hash,
    Ord,
    PartialOrd,
    Serialize_tuple,
    Deserialize_tuple,
    GetSize,
)]
pub struct BeaconEntry {
    round: u64,
    #[serde(with = "serde_byte_array")]
    signature: Vec<u8>,
}

impl BeaconEntry {
    pub fn new(round: u64, signature: Vec<u8>) -> Self {
        // Drop any excess capacity to make heap usage deterministic across allocators.
        // Avoid shrink_to_fit: it's a non-binding hint.
        let signature = signature.into_boxed_slice().into_vec();
        debug_assert_eq!(
            signature.len(),
            signature.capacity(),
            "BeaconEntry::signature should be right-sized"
        );
        Self { round, signature }
    }

    /// Returns the current round number.
    pub fn round(&self) -> u64 {
        self.round
    }

    /// The signature of message `H(prev_round.signature, round)` for `mainnet`
    /// or `H(round)` for `quicknet`.
    pub fn signature(&self) -> &[u8] {
        &self.signature
    }

    pub fn into_parts(self) -> (u64, Vec<u8>) {
        let Self { round, signature } = self;
        (round, signature)
    }

    // Hash the message: H(curr_round)
    pub fn message_unchained(round: u64) -> impl AsRef<[u8]> {
        let mut round_bytes = [0; std::mem::size_of::<u64>()];
        BigEndian::write_u64(&mut round_bytes, round);
        sha2::Sha256::digest(round_bytes)
    }

    // Hash the message: H(prev_sig | curr_round)
    pub fn message_chained(round: u64, prev_signature: impl AsRef<[u8]>) -> impl AsRef<[u8]> {
        let mut round_bytes = [0; std::mem::size_of::<u64>()];
        BigEndian::write_u64(&mut round_bytes, round);
        let mut hasher = sha2::Sha256::default();
        hasher.update(prev_signature);
        hasher.update(round_bytes);
        hasher.finalize()
    }
}
