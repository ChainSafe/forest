// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use crate::DelegatedConsensus;
use async_std::{
    sync::RwLock,
    task::{self, JoinHandle},
};
use forest_chain_sync::consensus::{MessagePoolApi, Proposer, SyncGossipSubmitter};
use forest_ipld_blockstore::BlockStore;
use forest_key_management::KeyStore;
use forest_state_manager::StateManager;
use futures::TryFutureExt;
use fvm_shared::{bigint::BigInt, FILECOIN_PRECISION};
use log::{error, info};
use std::sync::Arc;

type MiningTask = JoinHandle<anyhow::Result<()>>;

pub type FullConsensus = DelegatedConsensus;

pub const FETCH_PARAMS: bool = false;

// Reward 1FIL on top of the gas, which is what Eudico does.
pub fn reward_calc() -> Arc<dyn forest_interpreter::RewardCalc> {
    Arc::new(forest_interpreter::FixedRewardCalc {
        reward: BigInt::from(1) * FILECOIN_PRECISION,
    })
}

pub async fn consensus<DB, MP>(
    state_manager: &Arc<StateManager<DB>>,
    keystore: &Arc<RwLock<KeyStore>>,
    mpool: &Arc<MP>,
    submitter: SyncGossipSubmitter,
) -> (FullConsensus, Option<MiningTask>)
where
    DB: BlockStore + Send + Sync + 'static,
    MP: MessagePoolApi + Send + Sync + 'static,
{
    let consensus = DelegatedConsensus::default();
    if let Some(proposer) = consensus.proposer(keystore, state_manager).await.unwrap() {
        info!("Starting the delegated consensus proposer...");
        let sm = state_manager.clone();
        let mp = mpool.clone();
        let mining_task = task::spawn(async move {
            proposer
                .run(sm, mp.as_ref(), &submitter)
                .inspect_err(|e| error!("block proposal stopped: {}", e))
                .await
        });
        (consensus, Some(mining_task))
    } else {
        (consensus, None)
    }
}
