// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::shim::clock::ChainEpoch;
use fvm_shared2::consensus::ConsensusFault;
use fvm2::externs::{Consensus, Externs, Rand};

pub type ForestExternsV2 = super::externs::ForestExterns;

impl Externs for ForestExternsV2 {}

impl Rand for ForestExternsV2 {
    fn get_chain_randomness(&self, round: ChainEpoch) -> anyhow::Result<[u8; 32]> {
        crate::shim::externs::Rand::get_chain_randomness(self.rand.as_ref(), round)
    }

    fn get_beacon_randomness(&self, round: ChainEpoch) -> anyhow::Result<[u8; 32]> {
        crate::shim::externs::Rand::get_beacon_randomness(self.rand.as_ref(), round)
    }
}

impl Consensus for ForestExternsV2 {
    fn verify_consensus_fault(
        &self,
        h1: &[u8],
        h2: &[u8],
        extra: &[u8],
    ) -> anyhow::Result<(Option<ConsensusFault>, i64)> {
        Self::verify_consensus_fault(self, h1, h2, extra)
            .map(|(fault, gas_used)| (fault.map(From::from), gas_used))
    }
}
