// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::blocks::{Chain4U, HeaderBuilder, TipsetKey, chain4u};
use crate::chain::ChainStore;
use crate::db::MemoryDB;
use crate::networks::ChainConfig;
use crate::shim::clock::ChainEpoch;
use crate::shim::executor::{Receipt, StampedEvent};
use crate::state_manager::get_expensive_migration_heights;
use crate::utils::db::CborStoreExt;
use crate::utils::multihash::MultihashCode;
use cid::Cid;
use fil_actors_shared::fvm_ipld_amt::Amtv0 as Amt;
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

    let events_root = Amt::new_from_iter(&db, mock_event.clone()).unwrap();

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

    let receipt_root = Amt::new_from_iter(&db, vec![receipt]).unwrap();

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

#[test]
fn test_has_expensive_fork_between() {
    let TestChainSetup { state_manager, .. } = setup_chain_with_tipsets();
    let chain_config = state_manager.chain_config();

    let expensive_epoch = get_expensive_migration_heights(&chain_config.network)
        .iter()
        .find_map(|height| {
            let epoch = chain_config.epoch(*height);
            (epoch > 0).then_some(epoch)
        })
        .expect("expected at least one expensive migration epoch > 0");

    assert!(state_manager.has_expensive_fork_between(expensive_epoch, expensive_epoch + 1));
}
