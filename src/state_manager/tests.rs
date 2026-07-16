// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::db::MemoryDB;
use crate::shim::executor::StampedEvent;
use fil_actors_shared::fvm_ipld_amt::Amt;

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

#[test]
fn clear_tipset_state_caches_evicts_all_cached_results() {
    use crate::blocks::{CachingBlockHeader, RawBlockHeader};
    use crate::chain::ChainStore;
    use crate::networks::ChainConfig;
    use crate::shim::address::Address;

    let db = Arc::new(MemoryDB::default());
    let genesis = CachingBlockHeader::new(RawBlockHeader {
        miner_address: Address::new_id(0),
        timestamp: 7777,
        ..Default::default()
    });
    let cs = ChainStore::new(db, Arc::new(ChainConfig::default()), genesis).unwrap();
    let sm = StateManager::new(cs).unwrap();

    let tsk = sm.chain_store().heaviest_tipset().key().clone();
    sm.cache.insert(
        tsk.clone(),
        ExecutedTipset {
            state_root: Cid::default(),
            receipt_root: Cid::default(),
            executed_messages: Arc::new(vec![]),
        },
    );
    sm.trace_cache
        .insert(tsk.clone(), (Cid::default().into(), vec![]));
    assert!(sm.cache.get(&tsk).is_some());
    assert!(sm.trace_cache.get(&tsk).is_some());

    sm.clear_tipset_state_caches();
    assert!(sm.cache.get(&tsk).is_none());
    assert!(sm.trace_cache.get(&tsk).is_none());
}

#[test]
fn repair_tipset_lookup_clears_caches_when_entries_repaired() {
    use crate::blocks::{CachingBlockHeader, RawBlockHeader, Tipset};
    use crate::chain::ChainStore;
    use crate::db::EthMappingsStore;
    use crate::networks::ChainConfig;
    use crate::shim::address::Address;
    use crate::test_utils::dummy_ticket;
    use crate::utils::db::CborStoreExt;

    let db = Arc::new(MemoryDB::default());
    let genesis = CachingBlockHeader::new(RawBlockHeader {
        ticket: dummy_ticket(0),
        timestamp: 7777,
        ..Default::default()
    });
    db.put_cbor_default(&genesis).unwrap();
    let cs = ChainStore::new(db.clone(), Arc::new(ChainConfig::default()), genesis).unwrap();

    // Single-block chain crossing the lookup checkpoint at epoch 20.
    let mut head = cs.genesis_tipset();
    for epoch in 1..=21 {
        let ts = Tipset::from(CachingBlockHeader::new(RawBlockHeader {
            parents: head.key().clone(),
            ticket: dummy_ticket(epoch as u8),
            epoch,
            ..Default::default()
        }));
        for block in ts.block_headers() {
            db.put_cbor_default(block).unwrap();
        }
        head = ts;
    }
    cs.set_heaviest_tipset(head).unwrap();

    // Poison the checkpoint entry with a non-ancestor tipset.
    let fork = Tipset::from(CachingBlockHeader::new(RawBlockHeader {
        miner_address: Address::new_id(1),
        parents: cs.genesis_tipset().key().clone(),
        ticket: dummy_ticket(99),
        epoch: 20,
        ..Default::default()
    }));
    db.set_tipset_key_at_epoch(&fork).unwrap();

    let sm = StateManager::new(cs).unwrap();
    let tsk = sm.chain_store().heaviest_tipset().key().clone();
    sm.cache.insert(
        tsk.clone(),
        ExecutedTipset {
            state_root: Cid::default(),
            receipt_root: Cid::default(),
            executed_messages: Arc::new(vec![]),
        },
    );

    // The repair fixes the poisoned entry and evicts potentially tainted results.
    assert_eq!(sm.repair_tipset_lookup().unwrap(), 1);
    assert!(sm.cache.get(&tsk).is_none());

    // A clean follow-up scan leaves fresh cache content alone.
    sm.cache.insert(
        tsk.clone(),
        ExecutedTipset {
            state_root: Cid::default(),
            receipt_root: Cid::default(),
            executed_messages: Arc::new(vec![]),
        },
    );
    assert_eq!(sm.repair_tipset_lookup().unwrap(), 0);
    assert!(sm.cache.get(&tsk).is_some());
}
