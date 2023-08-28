// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::utils::encoding::serde_byte_array;
use serde_tuple::{self, Deserialize_tuple, Serialize_tuple};

/// The result from getting an entry from `Drand`.
/// The entry contains the round, or epoch as well as the BLS signature for that
/// round of randomness.
/// This beacon entry is stored on chain in the block header.
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize_tuple, Serialize_tuple)]
pub struct BeaconEntry {
    round: u64,
    #[serde(with = "serde_byte_array")]
    data: Vec<u8>,
}

impl BeaconEntry {
    pub fn new(round: u64, data: Vec<u8>) -> Self {
        Self { round, data }
    }
    /// Returns the current round number.
    pub fn round(&self) -> u64 {
        self.round
    }
    /// The signature of message `H(prev_round, prev_round.data, round)`.
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub fn into_parts(self) -> (u64, Vec<u8>) {
        let Self { round, data } = self;
        (round, data)
    }
}
