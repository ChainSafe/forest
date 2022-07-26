// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use anyhow::anyhow;
use async_std::sync::RwLock;
use async_trait::async_trait;
use key_management::KeyStore;
use std::fmt::Debug;
use std::str::FromStr;
use std::sync::Arc;
use thiserror::Error;

use chain::Error as ChainStoreError;
use chain::Scale;
use chain::Weight;
use chain_sync::consensus::Consensus;
use forest_blocks::{Block, Tipset};
use forest_encoding::Error as ForestEncodingError;
use fvm_shared::address::Address;
use fvm_shared::bigint::BigInt;
use ipld_blockstore::BlockStore;
use nonempty::NonEmpty;
use state_manager::Error as StateManagerError;
use state_manager::StateManager;

use crate::DelegatedProposer;

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
    chosen_one: Address,
}

impl Default for DelegatedConsensus {
    fn default() -> Self {
        Self {
            // The default _Miner ID_ assigned by Lotus will be `t01000` , because the miner sequence
            // starts from 1000. The corresponding default _Account ID_ will be `t0100`, which is the
            // first assigned by the system when it creates an account for the first miner in Genesis.
            // These will be two different `Actor` instances created for the Miner.
            //
            // In Eudico they use the _Account ID_ directly and not create a _Miner Actor_, but in
            // Forest we go through the common machinery, and validation will call [get_miner_work_addr],
            // which will treat the state pointed at by the `ActorState` as `miner::State`, so we _have_
            // to use the _Miner ID_ in this version, becuase the data would not deserialise as `account::State`.
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
        DB: BlockStore + Sync + Send + 'static,
    {
        let genesis = state_manager.chain_store().genesis()?;
        let genesis = genesis.ok_or_else(|| anyhow!("Genesis not set!"))?;
        let state_cid = genesis.state_root();
        let work_addr = state_manager.get_miner_work_addr(*state_cid, &self.chosen_one)?;

        match key_management::find_key(&work_addr, &*keystore.as_ref().read().await) {
            Ok(key) => Ok(Some(DelegatedProposer::new(self.chosen_one, key))),
            Err(key_management::Error::KeyInfo) => Ok(None),
            Err(e) => Err(anyhow!(e)),
        }
    }
}

impl Scale for DelegatedConsensus {
    fn weight<DB>(_: &DB, ts: &Tipset) -> anyhow::Result<Weight>
    where
        DB: BlockStore,
    {
        let header = ts.blocks().first().expect("Tipset is never empty.");
        // We don't have a height, only epoch, which is not exactly the same as there can be "null" epochs
        // without blocks. Maybe we can use the `ticket` field to maintain a height.
        // But since there can be only one block producer, it sounds like epoch should be fine to be used as weight.
        // After all if they wanted they could produce a series of empty blocks at each height and achieve the same weight.
        Ok(BigInt::from(header.epoch()))
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
        crate::validation::validate_block(&self.chosen_one, state_manager, block)
            .await
            .map_err(NonEmpty::new)
    }
}
