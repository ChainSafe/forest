// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use crate::FilecoinConsensus;
use forest_beacon::DrandBeacon;
use forest_chain_sync::consensus::{MessagePoolApi, SyncGossipSubmitter};
use forest_db::Store;
use forest_key_management::KeyStore;
use forest_state_manager::StateManager;
use fvm_ipld_blockstore::Blockstore;
use std::sync::Arc;
use tokio::{sync::RwLock, task::JoinSet};

pub type FullConsensus = FilecoinConsensus<DrandBeacon>;

pub const FETCH_PARAMS: bool = true;

pub fn reward_calc() -> Arc<dyn forest_interpreter::RewardCalc> {
    Arc::new(forest_interpreter::RewardActorMessageCalc)
}

#[allow(clippy::unused_async)]
pub async fn consensus<DB, MP>(
    state_manager: &Arc<StateManager<DB>>,
    _keystore: &Arc<RwLock<KeyStore>>,
    _mpool: &Arc<MP>,
    _submitter: SyncGossipSubmitter,
    _services: &mut JoinSet<anyhow::Result<()>>,
) -> anyhow::Result<FullConsensus>
where
    DB: Blockstore + Store + Clone + Send + Sync + 'static,
    MP: MessagePoolApi + Send + Sync + 'static,
{
    let consensus = FilecoinConsensus::new(state_manager.beacon_schedule());

    Ok(consensus)
}
