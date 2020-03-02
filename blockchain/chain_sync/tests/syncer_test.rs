// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use async_std::sync::channel;
use chain_sync::ChainSyncer;
use db::MemoryDB;

#[test]
fn chainsync_constructor() {
    let db = MemoryDB::default();
    let (local_sender, _test_receiver) = channel(20);
    let (_event_sender, event_receiver) = channel(20);

    // Test just makes sure that the chain syncer can be created without using a live database or
    // p2p network (local channels to simulate network messages and responses)
    let _chain_syncer = ChainSyncer::new(&db, local_sender, event_receiver).unwrap();
}
