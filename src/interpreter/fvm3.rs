// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::shim::clock::ChainEpoch;
use cid::Cid;
use fvm_shared3::consensus::ConsensusFault;
use fvm3::externs::{Chain, Consensus, Externs, Rand};

pub type ForestExternsV3 = super::externs::ForestExterns;

impl Externs for ForestExternsV3 {}

impl Rand for ForestExternsV3 {
    fn get_chain_randomness(&self, round: ChainEpoch) -> anyhow::Result<[u8; 32]> {
        crate::shim::externs::Rand::get_chain_randomness(self.rand.as_ref(), round)
    }

    fn get_beacon_randomness(&self, round: ChainEpoch) -> anyhow::Result<[u8; 32]> {
        crate::shim::externs::Rand::get_beacon_randomness(self.rand.as_ref(), round)
    }
}

impl Consensus for ForestExternsV3 {
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

impl Chain for ForestExternsV3 {
    fn get_tipset_cid(&self, epoch: ChainEpoch) -> anyhow::Result<Cid> {
        Self::get_tipset_cid(self, epoch)
    }
}
