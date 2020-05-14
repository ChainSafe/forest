// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use blocks::TipsetKeys;
use chain;
use clock::ChainEpoch;
use crypto::DomainSeparationTag;
use ipld_blockstore::BlockStore;
use std::error::Error;

#[derive(Debug, Clone)]
pub struct ChainRand {
    pub blks: TipsetKeys,
}

impl ChainRand {
    pub fn new(blks: TipsetKeys) -> Self {
        Self { blks }
    }
    pub fn get_randomness<DB: BlockStore>(
        &self,
        db: &DB,
        pers: DomainSeparationTag,
        round: ChainEpoch,
        entropy: &[u8],
    ) -> Result<Vec<u8>, Box<dyn Error>> {
        chain::get_randomness(db, &self.blks, pers, round, entropy)
    }
}
