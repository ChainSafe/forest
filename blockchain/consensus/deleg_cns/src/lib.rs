use address::Address;
// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use async_trait::async_trait;
use std::fmt::Debug;
use std::str::FromStr;
use std::sync::Arc;
use thiserror::Error;

use blocks::Block;
use chain::Error as ChainStoreError;
use chain_sync::Consensus;
use encoding::Error as ForestEncodingError;
use ipld_blockstore::BlockStore;
use nonempty::NonEmpty;
use state_manager::Error as StateManagerError;
use state_manager::StateManager;

mod validation;

#[derive(Debug, Error)]
pub enum DelegatedConsensusError {
    #[error("Block must not have an election proof")]
    BlockWithElectionProof,
    #[error("Block must not have a ticket")]
    BlockWithTicket,
    #[error("Block had the wrong timestamp: {0} != {1}")]
    UnequalBlockTimestamps(u64, u64),
    #[error("Miner isn't elligible to mine")]
    MinerNotEligibleToMine,
    #[error("Chain store error: {0}")]
    ChainStore(#[from] ChainStoreError),
    #[error("StateManager error: {0}")]
    StateManager(#[from] StateManagerError),
    #[error("Encoding error: {0}")]
    ForestEncoding(#[from] ForestEncodingError),
}

/// In Delegated Consensus only the chosen one can propose blocks.
///
/// This consensus is only used for demos.
#[derive(Debug)]
pub struct DelegatedConsensus {
    /// Address of the only miner eligible to propose blocks.
    ///
    /// Historically this has been hardcoded to `t0100`,
    /// which is the ID of the first actor created by the system.
    chosen_one: Address,
}

impl DelegatedConsensus {
    pub fn new(chosen_one: Address) -> Self {
        Self { chosen_one }
    }
}

impl Default for DelegatedConsensus {
    fn default() -> Self {
        Self {
            chosen_one: Address::from_str("t0100").unwrap(),
        }
    }
}

#[async_trait]
impl Consensus for DelegatedConsensus {
    type Error = DelegatedConsensusError;

    async fn validate_block<DB>(
        &self,
        state_manager: Arc<StateManager<DB>>,
        block: Arc<Block>,
    ) -> Result<(), NonEmpty<Self::Error>>
    where
        DB: BlockStore + Sync + Send + 'static,
    {
        validation::validate_block(&self.chosen_one, state_manager, block)
            .await
            .map_err(NonEmpty::new)
    }
}
