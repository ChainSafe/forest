// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::blocks::{Chain4U, HeaderBuilder, chain4u};
use crate::chain::ChainStore;
use crate::db::MemoryDB;
use crate::networks::ChainConfig;
use crate::shim::clock::ChainEpoch;
use crate::shim::executor::StampedEvent;
use crate::utils::db::CborStoreExt;
use crate::utils::multihash::MultihashCode;
use cid::Cid;
use fil_actors_shared::fvm_ipld_amt::Amt;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::DAG_CBOR;
use multihash_derive::MultihashDigest;
use num_bigint::BigInt;
use std::sync::Arc;

fn create_dummy_cid(i: u64) -> Cid {
    let bytes = i.to_le_bytes().to_vec();
    Cid::new_v1(DAG_CBOR, MultihashCode::Blake2b256.digest(&bytes))
}

fn dummy_state(db: impl Blockstore, i: ChainEpoch) -> Cid {
    db.put_cbor_default(&i).unwrap()
}

fn dummy_node(db: impl Blockstore, i: ChainEpoch) -> HeaderBuilder {
    HeaderBuilder {
        state_root: dummy_state(db, i).into(),
        weight: BigInt::from(i).into(),
        epoch: i.into(),
        timestamp: 100.into(),
        ..Default::default()
    }
}

/// Structure to hold the setup components for chain tests
struct TestChainSetup {
    chain_store: Arc<ChainStore<MemoryDB>>,
    chain_builder: Chain4U<Arc<MemoryDB>>,
    state_root: Cid,
    receipt_root: Cid,
}

fn setup_chain_with_tipsets() -> TestChainSetup {
    let db = Arc::new(MemoryDB::default());
    let chain_config = Arc::new(ChainConfig::default());

    let chain_builder = Chain4U::with_blockstore(db.clone());
    chain4u! {
        in chain_builder;
        [genesis_header = dummy_node(&db, 0)]
    }

    let chain_store = Arc::new(
        ChainStore::new(
            db.clone(),
            db.clone(),
            db.clone(),
            chain_config.clone(),
            genesis_header.clone().into(),
        )
        .expect("should create chain store"),
    );

    // Create dummy state and receipt roots and store them in blockstore
    let state_root = create_dummy_cid(1);
    let receipt_root = create_dummy_cid(2);

    db.put_keyed(&state_root, "dummy_state".as_bytes()).unwrap();
    db.put_keyed(&receipt_root, "dummy_receipt".as_bytes())
        .unwrap();

    chain_store
        .set_heaviest_tipset(chain_store.genesis_tipset())
        .unwrap();

    TestChainSetup {
        chain_store,
        chain_builder, // Assign c4u to the named field
        state_root,
        receipt_root,
    }
}

fn create_raw_event_v4(emitter: u64, key: &str) -> fvm_shared4::event::StampedEvent {
    fvm_shared4::event::StampedEvent {
        emitter,
        event: fvm_shared4::event::ActorEvent {
            entries: vec![fvm_shared4::event::Entry {
                flags: fvm_shared4::event::Flags::FLAG_INDEXED_ALL,
                key: key.to_string(),
                codec: fvm_ipld_encoding::IPLD_RAW,
                value: key.as_bytes().to_vec(),
            }],
        },
    }
}

fn create_raw_event_v3(emitter: u64, key: &str) -> fvm_shared3::event::StampedEvent {
    fvm_shared3::event::StampedEvent {
        emitter,
        event: fvm_shared3::event::ActorEvent {
            entries: vec![fvm_shared3::event::Entry {
                flags: fvm_shared3::event::Flags::FLAG_INDEXED_ALL,
                key: key.to_string(),
                codec: fvm_ipld_encoding::IPLD_RAW,
                value: key.as_bytes().to_vec(),
            }],
        },
    }
}

#[test]
fn test_events_store_and_retrieve_basic() {
    let db: MemoryDB = MemoryDB::default();

    // Create some test events
    let events = [
        create_raw_event_v4(1000, "event1"),
        create_raw_event_v4(1001, "event2"),
        create_raw_event_v4(1002, "event3"),
    ];

    // Store events in AMT with the same bitwidth as used in apply_block_messages
    let events_root =
        Amt::new_from_iter_with_bit_width(&db, EVENTS_AMT_BITWIDTH, events.iter()).unwrap();

    // Retrieve events from the AMT
    let retrieved_events = StampedEvent::get_events(&db, &events_root).unwrap();

    // Verify count matches
    assert_eq!(retrieved_events.len(), 3);

    // Verify content matches
    assert_eq!(retrieved_events[0].emitter(), 1000);
    assert_eq!(retrieved_events[1].emitter(), 1001);
    assert_eq!(retrieved_events[2].emitter(), 1002);
}

#[test]
fn test_events_entries_are_preserved_when_duplicates_are_stored() {
    let db = MemoryDB::default();

    // Create events with intentional duplicates (same content)
    let event1 = create_raw_event_v4(1001, "event1");
    let event2 = create_raw_event_v4(1002, "event2");
    let event3 = create_raw_event_v4(1003, "event3");

    let events = [event1.clone(), event1.clone(), event2, event3, event1];
    let events_root =
        Amt::new_from_iter_with_bit_width(&db, EVENTS_AMT_BITWIDTH, events.iter()).unwrap();
    let retrieved_events = StampedEvent::get_events(&db, &events_root).unwrap();
    assert_eq!(retrieved_events.len(), 5);
    // Verify the duplicates are at correct positions
    assert_eq!(retrieved_events[0].emitter(), 1001);
    assert_eq!(retrieved_events[1].emitter(), 1001); // duplicate
    assert_eq!(retrieved_events[2].emitter(), 1002);
    assert_eq!(retrieved_events[3].emitter(), 1003);
    assert_eq!(retrieved_events[4].emitter(), 1001); // non consecutive duplicate
}

#[test]
fn test_events_preserve_order() {
    let db = MemoryDB::default();

    // Create events with specific emitter IDs to track order
    let events = [
        create_raw_event_v4(100, "first"),
        create_raw_event_v4(200, "second"),
        create_raw_event_v4(300, "third"),
        create_raw_event_v4(400, "fourth"),
        create_raw_event_v4(500, "fifth"),
    ];
    let events_root =
        Amt::new_from_iter_with_bit_width(&db, EVENTS_AMT_BITWIDTH, events.iter()).unwrap();
    let retrieved_events = StampedEvent::get_events(&db, &events_root).unwrap();

    assert_eq!(retrieved_events.len(), 5);
    assert_eq!(retrieved_events[0].emitter(), 100);
    assert_eq!(retrieved_events[1].emitter(), 200);
    assert_eq!(retrieved_events[2].emitter(), 300);
    assert_eq!(retrieved_events[3].emitter(), 400);
    assert_eq!(retrieved_events[4].emitter(), 500);
}

#[test]
fn test_events_same_content_same_cid() {
    let db = MemoryDB::default();

    // Create identical event lists
    let events1 = [
        create_raw_event_v4(1000, "event_a"),
        create_raw_event_v4(1001, "event_b"),
    ];
    let events2 = [
        create_raw_event_v4(1000, "event_a"),
        create_raw_event_v4(1001, "event_b"),
    ];

    // Store both lists
    let root1 =
        Amt::new_from_iter_with_bit_width(&db, EVENTS_AMT_BITWIDTH, events1.iter()).unwrap();
    let root2 =
        Amt::new_from_iter_with_bit_width(&db, EVENTS_AMT_BITWIDTH, events2.iter()).unwrap();

    // Same content should produce same CID
    assert_eq!(
        root1, root2,
        "Identical events should produce identical CIDs"
    );
}

#[test]
fn test_events_empty_list() {
    let db = MemoryDB::default();

    let events: Vec<fvm_shared4::event::StampedEvent> = vec![];
    let events_root =
        Amt::new_from_iter_with_bit_width(&db, EVENTS_AMT_BITWIDTH, events.iter()).unwrap();

    let retrieved_events = StampedEvent::get_events(&db, &events_root).unwrap();
    assert!(
        retrieved_events.is_empty(),
        "Empty events list should return empty"
    );
}

#[test]
fn test_events_v3_store_and_retrieve() {
    let db = MemoryDB::default();

    let events = [
        create_raw_event_v3(2000, "v3_event1"),
        create_raw_event_v3(2001, "v3_event2"),
    ];

    // Store V3 events
    let events_root =
        Amt::new_from_iter_with_bit_width(&db, EVENTS_AMT_BITWIDTH, events.iter()).unwrap();
    let retrieved_events = StampedEvent::get_events(&db, &events_root).unwrap();

    assert_eq!(retrieved_events.len(), 2);
    assert_eq!(retrieved_events[0].emitter(), 2000);
    assert_eq!(retrieved_events[1].emitter(), 2001);
}

#[test]
fn test_identical_events_produce_same_root() {
    let db = MemoryDB::default();

    // Create identical event lists
    let events1 = [
        create_raw_event_v4(1000, "event_a"),
        create_raw_event_v4(1001, "event_b"),
    ];
    let events2 = [
        create_raw_event_v4(1000, "event_a"),
        create_raw_event_v4(1001, "event_b"),
    ];

    let root1 =
        Amt::new_from_iter_with_bit_width(&db, EVENTS_AMT_BITWIDTH, events1.iter()).unwrap();
    let root2 =
        Amt::new_from_iter_with_bit_width(&db, EVENTS_AMT_BITWIDTH, events2.iter()).unwrap();

    assert_eq!(root1, root2);
    let retrieved_events = StampedEvent::get_events(&db, &root1).unwrap();

    // Each AMT contains 2 events, and since roots are the same, we get 2 events
    assert_eq!(retrieved_events.len(), 2);
    assert_eq!(retrieved_events[0].emitter(), 1000);
    assert_eq!(retrieved_events[1].emitter(), 1001);
}
