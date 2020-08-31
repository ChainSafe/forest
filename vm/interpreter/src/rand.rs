// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use blocks::TipsetKeys;
use clock::ChainEpoch;
use crypto::DomainSeparationTag;
use ipld_blockstore::BlockStore;
use std::error::Error;

/// Randomness provider trait
pub trait Rand {
    /// Gets 32 bytes of randomness for ChainRand paramaterized by the DomainSeparationTag,
    /// ChainEpoch, Entropy from the ticket chain.
    fn get_chain_randomness<DB: BlockStore>(
        &self,
        db: &DB,
        pers: DomainSeparationTag,
        round: ChainEpoch,
        entropy: &[u8],
    ) -> Result<[u8; 32], Box<dyn Error>>;
    /// Gets 32 bytes of randomness for ChainRand paramaterized by the DomainSeparationTag,
    /// ChainEpoch, Entropy from the latest beacon entry.
    fn get_beacon_randomness<DB: BlockStore>(
        &self,
        db: &DB,
        pers: DomainSeparationTag,
        round: ChainEpoch,
        entropy: &[u8],
    ) -> Result<[u8; 32], Box<dyn Error>>;
}

/// Allows for deriving the randomness from a particular tipset
#[derive(Debug, Clone)]
pub struct ChainRand {
    pub blks: TipsetKeys,
}

impl ChainRand {
    /// Construct a new ChainRand
    pub fn new(blks: TipsetKeys) -> Self {
        Self { blks }
    }
}

impl Rand for ChainRand {
    fn get_chain_randomness<DB: BlockStore>(
        &self,
        db: &DB,
        pers: DomainSeparationTag,
        round: ChainEpoch,
        entropy: &[u8],
    ) -> Result<[u8; 32], Box<dyn Error>> {
        chain::get_chain_randomness(db, &self.blks, pers, round, entropy)
    }

    fn get_beacon_randomness<DB: BlockStore>(
        &self,
        db: &DB,
        pers: DomainSeparationTag,
        round: ChainEpoch,
        entropy: &[u8],
    ) -> Result<[u8; 32], Box<dyn Error>> {
        chain::get_beacon_randomness(db, &self.blks, pers, round, entropy)
    }
}
