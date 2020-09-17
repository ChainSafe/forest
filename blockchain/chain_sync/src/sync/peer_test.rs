// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use address::Address;
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

    let chain_store = Arc::new(ChainStore::new(db.clone()));

    let (local_sender, _test_receiver) = channel(20);
    let (event_sender, event_receiver) = channel(20);

    let msg_root = compute_msg_meta(chain_store.blockstore(), &[], &[]).unwrap();

    let dummy_header = BlockHeader::builder()
        .miner_address(Address::new_id(1000))
        .messages(msg_root)
        .message_receipts(Cid::new_from_cbor(&[1, 2, 3], Blake2b256))
        .state_root(Cid::new_from_cbor(&[1, 2, 3], Blake2b256))
        .build_and_validate()
        .unwrap();
    let gen_hash = chain_store.set_genesis(&dummy_header).unwrap();

    let genesis_ts = Arc::new(Tipset::new(vec![dummy_header]).unwrap());
    let beacon = Arc::new(MockBeacon::new(Duration::from_secs(1)));
    let state_manager = Arc::new(StateManager::new(db));
    let cs = ChainSyncer::new(
        chain_store,
        state_manager,
        beacon,
        local_sender,
        event_receiver,
        genesis_ts.clone(),
    )
    .unwrap();

    let peer_manager = Arc::clone(&cs.network.peer_manager_cloned());

    task::spawn(async {
        cs.start(0).await;
    });

    let source = PeerId::random();
    let source_clone = source.clone();
    let (sender, _) = channel(1);

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
                channel: ResponseChannel {
                    peer: source,
                    sender,
                },
            })
            .await;

        // Would be ideal to not have to sleep here and have it deterministic
        task::sleep(Duration::from_millis(1000)).await;

        assert_eq!(peer_manager.len().await, 1);
        assert_eq!(peer_manager.sorted_peers().await, &[source_clone]);
    });
}
