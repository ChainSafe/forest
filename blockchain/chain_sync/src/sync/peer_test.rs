// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use async_std::sync::channel;
use async_std::task;
use beacon::MockBeacon;
use blocks::BlockHeader;
use db::MemoryDB;
use forest_libp2p::{hello::HelloRequest, rpc::ResponseChannel};
use libp2p::core::PeerId;
use state_manager::StateManager;
use std::time::Duration;

#[test]
fn peer_manager_update() {
    let db = Arc::new(MemoryDB::default());

    let chain_store = ChainStore::new(db.clone());

    let (local_sender, _test_receiver) = channel(20);
    let (event_sender, event_receiver) = channel(20);

    let dummy_header = BlockHeader::builder()
        .miner_address(Address::new_id(1000))
        .messages(Cid::new_from_cbor(&[1, 2, 3], Blake2b256))
        .message_receipts(Cid::new_from_cbor(&[1, 2, 3], Blake2b256))
        .state_root(Cid::new_from_cbor(&[1, 2, 3], Blake2b256))
        .build()
        .unwrap();
    chain_store.set_genesis(dummy_header.clone()).unwrap();

    let genesis_ts = Tipset::new(vec![dummy_header]).unwrap();
    let beacon = Arc::new(MockBeacon::new(Duration::from_secs(1)));
    let state_manager = Arc::new(StateManager::new(db));
    let cs = ChainSyncer::new(
        chain_store,
        state_manager,
        beacon,
        local_sender,
        event_receiver,
        genesis_ts,
    )
    .unwrap();

    let peer_manager = Arc::clone(&cs.peer_manager);

    task::spawn(async {
        cs.start().await.unwrap();
    });

    let source = PeerId::random();
    let source_clone = source.clone();
    let (sender, _) = channel(1);

    task::block_on(async {
        event_sender
            .send(NetworkEvent::HelloRequest {
                request: HelloRequest::default(),
                channel: ResponseChannel {
                    peer: source,
                    sender,
                },
            })
            .await;

        // Would be ideal to not have to sleep here and have it deterministic
        task::sleep(Duration::from_millis(50)).await;

        assert_eq!(peer_manager.len().await, 1);
        assert_eq!(peer_manager.get_peer().await, Some(source_clone));
    });
}
