// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use actor::EPOCH_DURATION_SECONDS;
use async_std::task;
use beacon::{DrandBeacon, DrandPublic};
use chain::tipset_from_keys;
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

    let cids = load_car(db.as_ref(), EXPORT_SR_40.as_ref()).unwrap();

    let mut chain_store = ChainStore::new(db.clone());
    let state_manager = Arc::new(StateManager::new(db));

    // Initialize genesis using default (currently space-race) genesis
    let (genesis, _) = initialize_genesis(None, &mut chain_store, &state_manager).unwrap();
    let chain_store = Arc::new(chain_store);
    let genesis = Arc::new(genesis);

    let beacon = Arc::new(DrandBeacon::new(
        "https://pl-us.incentinet.drand.sh",
        DrandPublic{coefficient: hex::decode("8cad0c72c606ab27d36ee06de1d5b2db1faf92e447025ca37575ab3a8aac2eaae83192f846fc9e158bc738423753d000").unwrap()},
        genesis.blocks()[0].timestamp(),
        EPOCH_DURATION_SECONDS as u64,
    )
    .await
    .unwrap());

    let mut ts = tipset_from_keys(chain_store.blockstore(), &TipsetKeys::new(cids)).unwrap();
    while ts.epoch() > TEST_NUM {
        ts = chain_store.tipset_from_keys(ts.parents()).unwrap();
    }

    let fts = chain_store.fill_tipset(ts).unwrap();
    for block in fts.into_blocks() {
        println!("new block");
        task::block_on(SyncWorker::<_, _, FullVerifier>::validate_block(
            chain_store.clone(),
            state_manager.clone(),
            beacon.clone(),
            Arc::new(block),
        ))
        .unwrap();
    }
}
