// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use std::sync::Arc;

use crate::beacon::DrandBeacon;
use crate::chain_sync::consensus::{MessagePoolApi, SyncGossipSubmitter};
use crate::key_management::KeyStore;
use crate::state_manager::StateManager;
use fvm_ipld_blockstore::Blockstore;
use tokio::{sync::RwLock, task::JoinSet};

use crate::fil_cns::FilecoinConsensus;

pub type FullConsensus = FilecoinConsensus<DrandBeacon>;

pub const FETCH_PARAMS: bool = true;

pub fn reward_calc() -> Arc<dyn crate::interpreter::RewardCalc> {
    Arc::new(crate::interpreter::RewardActorMessageCalc)
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
    DB: Blockstore + Clone + Send + Sync + 'static,
    MP: MessagePoolApi + Send + Sync + 'static,
{
    let consensus = FilecoinConsensus::new(state_manager.beacon_schedule());

    Ok(consensus)
}
