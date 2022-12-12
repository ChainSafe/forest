// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use crate::DelegatedConsensus;
use forest_chain_sync::consensus::{MessagePoolApi, Proposer, SyncGossipSubmitter};
use forest_db::Store;
use forest_key_management::KeyStore;
use forest_state_manager::StateManager;
use fvm_ipld_blockstore::Blockstore;
use fvm_shared::econ::TokenAmount;
use log::info;
use std::sync::Arc;
use tokio::{sync::RwLock, task::JoinSet};

pub type FullConsensus = DelegatedConsensus;

pub const FETCH_PARAMS: bool = false;

// Reward 1FIL on top of the gas, which is what Eudico does.
pub fn reward_calc() -> Arc<dyn forest_interpreter::RewardCalc> {
    Arc::new(forest_interpreter::FixedRewardCalc {
        reward: TokenAmount::from_whole(1),
    })
}

pub async fn consensus<DB, MP>(
    state_manager: &Arc<StateManager<DB>>,
    keystore: &Arc<RwLock<KeyStore>>,
    mpool: &Arc<MP>,
    submitter: SyncGossipSubmitter,
    services: &mut JoinSet<anyhow::Result<()>>,
) -> anyhow::Result<FullConsensus>
where
    DB: Blockstore + Store + Clone + Send + Sync + 'static,
    MP: MessagePoolApi + Send + Sync + 'static,
{
    let consensus = DelegatedConsensus::default();
    if let Some(proposer) = consensus.proposer(keystore, state_manager).await.unwrap() {
        info!("Starting the delegated consensus proposer...");
        let sm = state_manager.clone();
        let mp = mpool.clone();
        proposer.spawn(sm, mp, submitter, services).await?;
    }
    Ok(consensus)
}
