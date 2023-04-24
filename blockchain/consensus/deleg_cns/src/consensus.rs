// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use std::{fmt::Debug, str::FromStr, sync::Arc};

use anyhow::anyhow;
use async_trait::async_trait;
use forest_blocks::{Block, Tipset};
use forest_chain::{Error as ChainStoreError, Scale, Weight};
use forest_chain_sync::consensus::Consensus;
use forest_key_management::KeyStore;
use forest_shim::address::Address;
use forest_state_manager::{Error as StateManagerError, StateManager};
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::Error as ForestEncodingError;
use log::info;
use nonempty::NonEmpty;
use num::BigInt;
use thiserror::Error;
use tokio::sync::RwLock;

use crate::DelegatedProposer;

#[derive(Debug, Error)]
pub enum DelegatedConsensusError {
    #[error("Block must not have an election proof")]
    BlockWithElectionProof,
    #[error("Block must not have a ticket")]
    BlockWithTicket,
    #[error("Block had the wrong timestamp: {0} != {1}")]
    UnequalBlockTimestamps(u64, u64),
    #[error("Miner isn't eligible to mine: expected {0}; found {1}")]
    MinerNotEligibleToMine(Address, Address),
    #[error("Unknown miner: {0}")]
    UnknownMiner(Address),
    #[error("Chain store error: {0}")]
    ChainStore(#[from] ChainStoreError),
    #[error("StateManager error: {0}")]
    StateManager(#[from] StateManagerError),
    #[error("Encoding error: {0}")]
    ForestEncoding(#[from] ForestEncodingError),
}

impl From<forest_chain::Error> for Box<DelegatedConsensusError> {
    fn from(err: forest_chain::Error) -> Self {
        Box::new(Into::into(err))
    }
}

impl From<forest_state_manager::Error> for Box<DelegatedConsensusError> {
    fn from(err: forest_state_manager::Error) -> Self {
        Box::new(Into::into(err))
    }
}

/// In Delegated Consensus only the chosen one can propose blocks.
///
/// This consensus is only used for demos.
#[derive(Debug)]
pub struct DelegatedConsensus {
    /// Address of the only miner eligible to propose blocks.
    chosen_one: Address,
}

impl Default for DelegatedConsensus {
    fn default() -> Self {
        Self {
            // The default _Miner ID_ assigned by Lotus will be `t01000` , because the miner
            // sequence starts from 1000. The corresponding default _Account ID_ will be
            // `t0100`, which is the first assigned by the system when it creates an
            // account for the first miner in Genesis. These will be two different
            // `Actor` instances created for the Miner.
            //
            // In Eudico they use the _Account ID_ directly and not create a _Miner Actor_, but in
            // Forest we go through the common machinery, and validation will call
            // [get_miner_work_addr], which will treat the state pointed at by the
            // `ActorState` as `miner::State`, so we _have_ to use the _Miner ID_ in
            // this version, because the data would not deserialise as `account::State`.
            chosen_one: Address::from_str("t01000").unwrap(),
        }
    }
}

impl DelegatedConsensus {
    pub fn new(chosen_one: Address) -> Self {
        Self { chosen_one }
    }

    /// Create an instance of the proposer on the node
    /// which has the private key to sign blocks.
    ///
    /// If the key is not found in the `keystore`, then
    /// we assume this is *not* the node which should
    /// be doing the proposing and nothing is returned.
    pub async fn proposer<DB>(
        &self,
        keystore: &Arc<RwLock<KeyStore>>,
        state_manager: &Arc<StateManager<DB>>,
    ) -> anyhow::Result<Option<DelegatedProposer>>
    where
        DB: Blockstore + Clone + Sync + Send + 'static,
    {
        let genesis = state_manager.chain_store().genesis()?;
        let state_cid = genesis.state_root();
        let work_addr = state_manager.get_miner_work_addr(*state_cid, &self.chosen_one)?;

        info!(
            "The work address of the chosen proposer {} is {}",
            self.chosen_one, work_addr
        );

        match forest_key_management::find_key(&work_addr, &*keystore.as_ref().read().await) {
            Ok(key) => Ok(Some(DelegatedProposer::new(self.chosen_one, key))),
            Err(forest_key_management::Error::KeyInfo) => Ok(None),
            Err(e) => Err(anyhow!(e)),
        }
    }
}

impl Scale for DelegatedConsensus {
    fn weight<DB>(_: &DB, ts: &Tipset) -> anyhow::Result<Weight>
    where
        DB: Blockstore,
    {
        let header = ts.blocks().first().expect("Tipset is never empty.");
        // We don't have a height, only epoch, which is not exactly the same as there
        // can be "null" epochs without blocks. Maybe we can use the `ticket`
        // field to maintain a height. But since there can be only one block
        // producer, it sounds like epoch should be fine to be used as weight.
        // After all if they wanted they could produce a series of empty blocks at each
        // height and achieve the same weight.
        Ok(BigInt::from(header.epoch()))
    }
}

#[async_trait]
impl Consensus for DelegatedConsensus {
    type Error = Box<DelegatedConsensusError>;

    async fn validate_block<DB>(
        &self,
        state_manager: Arc<StateManager<DB>>,
        block: Arc<Block>,
    ) -> Result<(), NonEmpty<Self::Error>>
    where
        DB: Blockstore + Clone + Sync + Send + 'static,
    {
        crate::validation::validate_block(&self.chosen_one, state_manager, block)
            .await
            .map_err(NonEmpty::new)
    }
}
