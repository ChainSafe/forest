// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use async_std::task;
use clock::ChainEpoch;
use db::MemoryDB;
use fil_types::verifier::FullVerifier;
use forest_car::load_car;
use genesis::{initialize_genesis, EXPORT_SR_40};
use state_manager::StateManager;

// Change this to test different blocks
const TEST_NUM: ChainEpoch = 40;

#[async_std::test]
// Ignored because it depends on proof parameters for full verification
#[ignore]
async fn validate_specific_block() {
    pretty_env_logger::init();

    let db = Arc::new(MemoryDB::default());

    let cids = load_car(db.as_ref(), EXPORT_SR_40.as_ref()).await.unwrap();

    let chain_store = Arc::new(ChainStore::new(db.clone()));
    let state_manager = Arc::new(StateManager::new(chain_store.clone()));

    // Initialize genesis using default (currently space-race) genesis
    let (genesis, _) = initialize_genesis(None, &state_manager).await.unwrap();
    let genesis = Arc::new(genesis);

    let beacon = Arc::new(
        networks::beacon_schedule_default(genesis.min_timestamp())
            .await
            .unwrap(),
    );

    let mut ts = chain_store
        .tipset_from_keys(&TipsetKeys::new(cids))
        .await
        .unwrap();
    while ts.epoch() > TEST_NUM {
        ts = chain_store.tipset_from_keys(ts.parents()).await.unwrap();
    }

    let fts = chain_store.fill_tipset(&ts).unwrap();
    for block in fts.into_blocks() {
        task::block_on(SyncWorker::<_, _, FullVerifier>::validate_block(
            state_manager.clone(),
            beacon.clone(),
            Arc::new(block),
        ))
        .unwrap();
    }
}
