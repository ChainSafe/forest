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
fn space_race_full_sync() {
    let db = Arc::new(MemoryDB::default());

    let chain_store = Arc::new(ChainStore::new(db.clone()));

    // let (local_sender, _test_receiver) = channel(20);
    // let (event_sender, event_receiver) = channel(20);

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

    // let worker = 
}