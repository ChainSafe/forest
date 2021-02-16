// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use address::Address;
use async_std::channel::bounded;
use async_std::task;
use beacon::{BeaconPoint, MockBeacon};
use blocks::BlockHeader;
use db::MemoryDB;
use fil_types::verifier::MockVerifier;
use forest_libp2p::hello::HelloRequest;
use libp2p::core::PeerId;
use message_pool::{test_provider::TestApi, MessagePool};
use state_manager::StateManager;
use std::time::Duration;

#[test]
fn peer_manager_update() {
    let db = Arc::new(MemoryDB::default());

    let chain_store = Arc::new(ChainStore::new(db.clone()));
    let (tx, _rx) = bounded(10);
    let mpool = task::block_on(MessagePool::new(
        TestApi::default(),
        "test".to_string(),
        tx,
        Default::default(),
    ))
    .unwrap();
    let mpool = Arc::new(mpool);

    let (local_sender, _test_receiver) = bounded(20);
    let (event_sender, event_receiver) = bounded(20);

    let msg_root = compute_msg_meta(chain_store.blockstore(), &[], &[]).unwrap();

    let dummy_header = BlockHeader::builder()
        .miner_address(Address::new_id(1000))
        .messages(msg_root)
        .message_receipts(cid::new_from_cbor(&[1, 2, 3], Blake2b256))
        .state_root(cid::new_from_cbor(&[1, 2, 3], Blake2b256))
        .build()
        .unwrap();
    let gen_hash = chain_store.set_genesis(&dummy_header).unwrap();

    let genesis_ts = Arc::new(Tipset::new(vec![dummy_header]).unwrap());
    let beacon = Arc::new(BeaconSchedule(vec![BeaconPoint {
        height: 0,
        beacon: Arc::new(MockBeacon::new(Duration::from_secs(1))),
    }]));
    let state_manager = Arc::new(StateManager::new(chain_store));
    let cs = ChainSyncer::<_, _, MockVerifier, TestApi>::new(
        state_manager,
        beacon,
        mpool,
        local_sender,
        event_receiver,
        genesis_ts.clone(),
        SyncConfig::new(200, 0),
    )
    .unwrap();

    let peer_manager = Arc::clone(&cs.network.peer_manager.clone());

    let (worker_tx, worker_rx) = bounded(10);
    task::spawn(async {
        cs.start(worker_tx, worker_rx).await;
    });

    let source = PeerId::random();
    let source_clone = source.clone();

    let gen_cloned = genesis_ts.clone();
    task::block_on(async {
        event_sender
            .send(NetworkEvent::HelloRequest {
                request: HelloRequest {
                    heaviest_tip_set: gen_cloned.key().cids().to_vec(),
                    heaviest_tipset_height: gen_cloned.epoch(),
                    heaviest_tipset_weight: gen_cloned.weight().clone(),
                    genesis_hash: gen_hash,
                },
                source,
            })
            .await
            .unwrap();

        // Would be ideal to not have to sleep here and have it deterministic
        task::sleep(Duration::from_millis(1000)).await;

        assert_eq!(peer_manager.len().await, 1);
        assert_eq!(peer_manager.sorted_peers().await, &[source_clone]);
    });
}
