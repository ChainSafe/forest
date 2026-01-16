// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::blocks::{Chain4U, HeaderBuilder, TipsetKey, chain4u};
use crate::chain::ChainStore;
use crate::db::MemoryDB;
use crate::networks::ChainConfig;
use crate::shim::clock::ChainEpoch;
use crate::shim::executor::{Receipt, StampedEvent};
use crate::utils::db::CborStoreExt;
use crate::utils::multihash::MultihashCode;
use cid::Cid;
use fil_actors_shared::fvm_ipld_amt::{Amt, Amtv0};
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
    db: Arc<MemoryDB>,
    chain_store: Arc<ChainStore<MemoryDB>>,
    state_manager: Arc<StateManager<MemoryDB>>,
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

    let state_manager = Arc::new(StateManager::new(chain_store.clone()).unwrap());

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
        db,
        chain_store,
        state_manager,
        chain_builder, // Assign c4u to the named field
        state_root,
        receipt_root,
    }
}

#[test]
fn test_try_lookup_state_from_next_tipset_success() {
    let TestChainSetup {
        chain_store,
        chain_builder,
        state_root,
        receipt_root,
        ..
    } = setup_chain_with_tipsets();

    // Build a chain with parent and child tipsets
    chain4u! {
        in chain_builder;
        parent_ts @ [
            a = HeaderBuilder::new()
                .with_epoch(10)
                .with_timestamp(101)
        ]->
        child_ts @ [
            child_block = HeaderBuilder::new()
                .with_epoch(11)
                .with_parents(parent_ts.key().clone())
                .with_state_root(state_root)
                .with_message_receipts(receipt_root)
                .with_timestamp(102)
        ]
    }

    assert_eq!(a.epoch, 10);
    // parent state root is not set, so it should be empty
    assert_eq!(a.state_root, Cid::default());
    assert_eq!(child_block.epoch, 11);
    assert_eq!(child_block.state_root, state_root);

    chain_store.set_heaviest_tipset(child_ts.clone()).unwrap();

    let state_manager = Arc::new(StateManager::new(chain_store).unwrap());

    let result = state_manager.try_lookup_state_from_next_tipset(parent_ts);

    assert!(result.is_some());
    let state_output = result.unwrap();
    assert_eq!(state_output.state_root, state_root);
    assert_eq!(state_output.receipt_root, receipt_root);
}

#[test]
fn test_try_lookup_state_from_next_tipset_no_next_tipset() {
    let TestChainSetup {
        chain_store,
        chain_builder,
        ..
    } = setup_chain_with_tipsets();

    // Build a chain with just one tipset
    chain4u! {
        in chain_builder;
        a_ts @ [
            a = HeaderBuilder::new()
                .with_epoch(10)
        ]
    }

    assert_eq!(a.epoch, 10);

    chain_store.set_heaviest_tipset(a_ts.clone()).unwrap();

    let state_manager = Arc::new(StateManager::new(chain_store).unwrap());

    let result = state_manager.try_lookup_state_from_next_tipset(a_ts);

    // Should return None since there's no next tipset
    assert!(result.is_none());
}

#[test]
fn test_try_lookup_state_from_next_tipset_different_parent() {
    let TestChainSetup {
        chain_store,
        chain_builder,
        state_root,
        receipt_root,
        ..
    } = setup_chain_with_tipsets();

    // genesis -> a
    chain4u! {
        in chain_builder;
        a_ts @ [
            a = HeaderBuilder::new()
                .with_epoch(10)
                .with_timestamp(101) // genesis timestamp(100) + 1
        ]
    }

    // Build a chain with parent and child tipsets, but child has different parent
    // genesis -> a -> b
    //            \a1 --> b
    chain4u! {
        in chain_builder;
        // Different parent (a1)
        a1_ts @ [
            a1 = HeaderBuilder::new()
                .with_epoch(10)
                .with_timestamp(102) // genesis timestamp(100) + 2
        ]->
        b_ts @ [
            b = HeaderBuilder::new()
                .with_epoch(11)
                .with_parents(a1_ts.key().clone())
                .with_state_root(state_root)
                .with_message_receipts(receipt_root)
        ]
    }

    assert_eq!(a.epoch, 10);
    assert_eq!(a1.epoch, 10);
    assert_eq!(b.epoch, 11);

    // a tipset key should be different from `a1` tipset key
    assert_ne!(a_ts.key(), a1_ts.key());

    chain_store.set_heaviest_tipset(b_ts.clone()).unwrap();

    let state_manager = Arc::new(StateManager::new(chain_store).unwrap());

    let result = state_manager.try_lookup_state_from_next_tipset(a_ts);

    // Should return None since the child tipset (b_ts) has a different parent (a1_ts)
    assert!(result.is_none());
}

#[test]
fn test_try_lookup_state_from_next_tipset_missing_receipt_root() {
    let TestChainSetup {
        chain_store,
        chain_builder,
        state_root,
        ..
    } = setup_chain_with_tipsets();

    // Create a new receipt root that isn't stored in the blockstore
    let missing_receipt_root = create_dummy_cid(999);

    // Build a chain with parent and child tipsets
    chain4u! {
        in chain_builder;
        a_ts @ [
            a = HeaderBuilder::new()
                .with_epoch(10)
        ]->
        b_ts @ [
            b = HeaderBuilder::new()
                .with_epoch(11)
                .with_parents(a_ts.key().clone())
                .with_state_root(state_root)
                .with_message_receipts(missing_receipt_root)
        ]
    }

    assert_eq!(a.epoch, 10);
    assert_eq!(b.epoch, 11);

    chain_store.set_heaviest_tipset(b_ts.clone()).unwrap();

    let state_manager = Arc::new(StateManager::new(chain_store).unwrap());

    let result = state_manager.try_lookup_state_from_next_tipset(a_ts);

    // Should return None since the receipt root is missing
    assert!(result.is_none());
}

#[test]
fn test_try_lookup_state_from_next_tipset_missing_state_root() {
    let TestChainSetup {
        chain_store,
        chain_builder,
        receipt_root,
        ..
    } = setup_chain_with_tipsets();

    // Create a new state root that is not stored in the blockstore
    let missing_state_root = create_dummy_cid(999);

    // Build a chain with parent and child tipsets
    chain4u! {
        in chain_builder;
        a_ts @ [
            a = HeaderBuilder::new()
                .with_epoch(10)
        ]->
        b_ts @ [
            b = HeaderBuilder::new()
                .with_epoch(11)
                .with_parents(a_ts.key().clone())
                .with_message_receipts(receipt_root)
                .with_state_root(missing_state_root)
        ]
    }

    assert_eq!(a.epoch, 10);
    assert_eq!(b.epoch, 11);

    chain_store.set_heaviest_tipset(b_ts.clone()).unwrap();

    let state_manager = Arc::new(StateManager::new(chain_store).unwrap());

    let result = state_manager.try_lookup_state_from_next_tipset(a_ts);

    // Should return None since the state root is missing
    assert!(result.is_none());
}
#[test]
fn test_update_receipt_and_events_cache_empty_events() {
    let TestChainSetup { state_manager, .. } = setup_chain_with_tipsets();
    let tipset_key = TipsetKey::from(nunny::vec![create_dummy_cid(1)]);

    // Create state output with empty events
    let state_output = StateOutput {
        state_root: create_dummy_cid(2),
        receipt_root: create_dummy_cid(3),
        events: Vec::new(),
        events_roots: Vec::new(),
    };

    state_manager.update_cache_with_state_output(&tipset_key, &state_output);

    // Verify events cache wasn't updated
    assert!(
        state_manager
            .receipt_event_cache_handler
            .get_events(&tipset_key)
            .is_none()
    );
    assert!(
        state_manager
            .receipt_event_cache_handler
            .get_receipts(&tipset_key)
            .is_none()
    );
}

#[test]
fn test_update_receipt_and_events_cache_with_events() {
    let TestChainSetup {
        db, state_manager, ..
    } = setup_chain_with_tipsets();
    let tipset_key = TipsetKey::from(nunny::vec![create_dummy_cid(1)]);

    let mock_event = vec![StampedEvent::V4(fvm_shared4::event::StampedEvent {
        emitter: 1000,
        event: fvm_shared4::event::ActorEvent { entries: vec![] },
    })];

    let events_root = Amtv0::new_from_iter(&db, mock_event.clone()).unwrap();

    // Create state output with non-empty events
    let state_output = StateOutput {
        state_root: create_dummy_cid(2),
        receipt_root: create_dummy_cid(3),
        events: vec![mock_event],
        events_roots: vec![Some(events_root)],
    };

    state_manager.update_cache_with_state_output(&tipset_key, &state_output);

    // Verify events cache was updated
    let cached_events = state_manager
        .receipt_event_cache_handler
        .get_events(&tipset_key);
    assert!(cached_events.is_some());
    let events = cached_events.unwrap();
    assert_eq!(events.events.len(), 1);
    assert_eq!(events.roots.len(), 1);
}

#[test]
fn test_update_receipt_and_events_cache_receipts_success() {
    let TestChainSetup {
        db, state_manager, ..
    } = setup_chain_with_tipsets();
    let tipset_key = TipsetKey::from(nunny::vec![create_dummy_cid(1)]);

    // Create dummy receipt data
    let receipt = Receipt::V4(fvm_shared4::receipt::Receipt {
        exit_code: fvm_shared4::error::ExitCode::new(0),
        return_data: fvm_ipld_encoding::RawBytes::default(),
        gas_used: 100,
        events_root: None,
    });

    let receipt_root = Amtv0::new_from_iter(&db, vec![receipt]).unwrap();

    let state_output = StateOutput {
        state_root: create_dummy_cid(2),
        receipt_root,
        events: Vec::new(),
        events_roots: Vec::new(),
    };

    state_manager.update_cache_with_state_output(&tipset_key, &state_output);

    // Verify the receipt cache was updated
    let cached_receipts = state_manager
        .receipt_event_cache_handler
        .get_receipts(&tipset_key);
    assert!(cached_receipts.is_some());
    let receipts = cached_receipts.unwrap();
    assert_eq!(receipts.len(), 1);
}

#[test]
fn test_update_receipt_and_events_cache_receipts_failure() {
    let TestChainSetup { state_manager, .. } = setup_chain_with_tipsets();
    let tipset_key = TipsetKey::from(nunny::vec![create_dummy_cid(1)]);
    let receipt_root = create_dummy_cid(3);

    let state_output = StateOutput {
        state_root: create_dummy_cid(2),
        receipt_root,
        events: Vec::new(),
        events_roots: Vec::new(),
    };

    state_manager.update_cache_with_state_output(&tipset_key, &state_output);

    assert!(
        state_manager
            .receipt_event_cache_handler
            .get_receipts(&tipset_key)
            .is_none()
    );
}

#[test]
fn test_state_output_get_size() {
    let s = StateOutputValue::default();
    assert_eq!(s.get_size(), std::mem::size_of_val(&s));
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
