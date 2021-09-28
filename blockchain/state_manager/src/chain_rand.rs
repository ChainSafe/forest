// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use async_std::task;
use blockstore::BlockStore;
use chain::ChainStore;
use clock::ChainEpoch;
use forest_blocks::TipsetKeys;
use forest_crypto::DomainSeparationTag;
use interpreter::Rand;
use std::error::Error;
use std::sync::Arc;

/// Allows for deriving the randomness from a particular tipset.
#[derive(Clone)]
pub struct ChainRand<DB> {
    blks: TipsetKeys,
    cs: Arc<ChainStore<DB>>,
}

impl<DB> ChainRand<DB> {
    pub fn new(blks: TipsetKeys, cs: Arc<ChainStore<DB>>) -> Self {
        Self { blks, cs }
    }
}

impl<DB> Rand for ChainRand<DB>
where
    DB: BlockStore + Send + Sync + 'static,
{
    fn get_chain_randomness(
        &self,
        pers: DomainSeparationTag,
        round: ChainEpoch,
        entropy: &[u8],
    ) -> Result<[u8; 32], Box<dyn Error>> {
        task::block_on(
            self.cs
                .get_chain_randomness_looking_backward(&self.blks, pers, round, entropy),
        )
    }

    fn get_beacon_randomness(
        &self,
        pers: DomainSeparationTag,
        round: ChainEpoch,
        entropy: &[u8],
    ) -> Result<[u8; 32], Box<dyn Error>> {
        task::block_on(
            self.cs
                .get_beacon_randomness_looking_backward(&self.blks, pers, round, entropy),
        )
    }

    fn get_chain_randomness_looking_forward(
        &self,
        pers: DomainSeparationTag,
        round: ChainEpoch,
        entropy: &[u8],
    ) -> Result<[u8; 32], Box<dyn Error>> {
        task::block_on(
            self.cs
                .get_chain_randomness_looking_forward(&self.blks, pers, round, entropy),
        )
    }

    fn get_beacon_randomness_looking_forward(
        &self,
        pers: DomainSeparationTag,
        round: ChainEpoch,
        entropy: &[u8],
    ) -> Result<[u8; 32], Box<dyn Error>> {
        task::block_on(
            self.cs
                .get_beacon_randomness_looking_forward(&self.blks, pers, round, entropy),
        )
    }
}
