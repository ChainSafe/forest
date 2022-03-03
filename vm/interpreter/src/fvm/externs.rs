// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use crate::Rand;
use clock::ChainEpoch;
use crypto::DomainSeparationTag;
use fvm::externs::Consensus;
use fvm::externs::Externs;
use fvm_shared::consensus::ConsensusFault;

pub struct ForestExterns {
    rand: Box<dyn Rand>,
}

impl ForestExterns {
    pub fn new(rand: impl Rand + 'static) -> Self {
        ForestExterns {
            rand: Box::new(rand),
        }
    }
}

impl Externs for ForestExterns {}

impl Rand for ForestExterns {
    fn get_chain_randomness(
        &self,
        pers: DomainSeparationTag,
        round: ChainEpoch,
        entropy: &[u8],
    ) -> anyhow::Result<[u8; 32]> {
        self.rand.get_chain_randomness(pers, round, entropy)
    }

    fn get_beacon_randomness(
        &self,
        pers: DomainSeparationTag,
        round: ChainEpoch,
        entropy: &[u8],
    ) -> anyhow::Result<[u8; 32]> {
        self.rand.get_beacon_randomness(pers, round, entropy)
    }
}

impl Consensus for ForestExterns {
    fn verify_consensus_fault(
        &self,
        _h1: &[u8],
        _h2: &[u8],
        _extra: &[u8],
    ) -> anyhow::Result<(Option<ConsensusFault>, i64)> {
        panic!("Forest cannot verify consensus faults. Please report this to https://github.com/ChainSafe/forest/issues")
    }
}
