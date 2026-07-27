// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::chain::index::tests::{genesis_tipset, persist_tipset, tipset_child};

#[test]
fn test_validate_tipset_lookup_hamt_success() {
    let db = Arc::new(MemoryDB::default());
    let mut hamt: Hamt<_, TipsetKey, ChainEpoch> =
        Hamt::new_with_bit_width(db.shallow_clone(), TIPSET_LOOKUP_HAMT_BIT_WIDTH);
    let genesis = genesis_tipset();
    persist_tipset(&genesis, &db);
    // Epoch 20 is a null round: 19 is followed directly by 21.
    let mut prev = genesis.shallow_clone();
    for epoch in [10, 19, 21, 30, 40, 41] {
        let ts = tipset_child(&prev, epoch);
        if ChainIndex::is_tipset_lookup_checkpoint(epoch) {
            hamt.set(epoch, ts.key().clone()).unwrap();
        }
        persist_tipset(&ts, &db);
        prev = ts;
    }
    let head = prev;
    let hamt_root = hamt.flush().unwrap();
    validate_tipset_lookup_hamt(&db, hamt_root, head).unwrap();
}

#[test]
fn test_validate_tipset_lookup_hamt_non_checkpoint_entry() {
    let db = Arc::new(MemoryDB::default());
    let mut hamt: Hamt<_, TipsetKey, ChainEpoch> =
        Hamt::new_with_bit_width(db.shallow_clone(), TIPSET_LOOKUP_HAMT_BIT_WIDTH);
    let genesis = genesis_tipset();
    persist_tipset(&genesis, &db);
    // Epoch 20 is a null round: 19 is followed directly by 21.
    let mut prev = genesis.shallow_clone();
    for epoch in [10, 19, 21, 30, 40, 41] {
        let ts = tipset_child(&prev, epoch);
        hamt.set(epoch, ts.key().clone()).unwrap();
        persist_tipset(&ts, &db);
        prev = ts;
    }
    let head = prev;
    let hamt_root = hamt.flush().unwrap();
    validate_tipset_lookup_hamt(&db, hamt_root, head).unwrap_err();
}

#[test]
fn test_validate_tipset_lookup_hamt_missing_checkpoint_entry() {
    let db = Arc::new(MemoryDB::default());
    let mut hamt: Hamt<_, TipsetKey, ChainEpoch> =
        Hamt::new_with_bit_width(db.shallow_clone(), TIPSET_LOOKUP_HAMT_BIT_WIDTH);
    let genesis = genesis_tipset();
    persist_tipset(&genesis, &db);
    // Epoch 20 is a null round: 19 is followed directly by 21.
    let mut prev = genesis.shallow_clone();
    for epoch in [10, 19, 21, 30, 40, 41] {
        let ts = tipset_child(&prev, epoch);
        persist_tipset(&ts, &db);
        prev = ts;
    }
    let head = prev;
    let hamt_root = hamt.flush().unwrap();
    validate_tipset_lookup_hamt(&db, hamt_root, head).unwrap_err();
}

#[test]
fn test_validate_tipset_lookup_hamt_bad_checkpoint_entry() {
    let db = Arc::new(MemoryDB::default());
    let mut hamt: Hamt<_, TipsetKey, ChainEpoch> =
        Hamt::new_with_bit_width(db.shallow_clone(), TIPSET_LOOKUP_HAMT_BIT_WIDTH);
    let genesis = genesis_tipset();
    persist_tipset(&genesis, &db);
    hamt.set(40, genesis.key().clone()).unwrap();
    // Epoch 20 is a null round: 19 is followed directly by 21.
    let mut prev = genesis.shallow_clone();
    for epoch in [10, 19, 21, 30, 40, 41] {
        let ts = tipset_child(&prev, epoch);
        persist_tipset(&ts, &db);
        prev = ts;
    }
    let head = prev;
    let hamt_root = hamt.flush().unwrap();
    validate_tipset_lookup_hamt(&db, hamt_root, head).unwrap_err();
}

#[test]
fn test_validate_tipset_lookup_hamt_null_checkpoint_entry() {
    let db = Arc::new(MemoryDB::default());
    let mut hamt: Hamt<_, TipsetKey, ChainEpoch> =
        Hamt::new_with_bit_width(db.shallow_clone(), TIPSET_LOOKUP_HAMT_BIT_WIDTH);
    let genesis = genesis_tipset();
    persist_tipset(&genesis, &db);
    hamt.set(20, genesis.key().clone()).unwrap();
    // Epoch 20 is a null round: 19 is followed directly by 21.
    let mut prev = genesis.shallow_clone();
    for epoch in [10, 19, 21, 30, 40, 41] {
        let ts = tipset_child(&prev, epoch);
        persist_tipset(&ts, &db);
        prev = ts;
    }
    let head = prev;
    let hamt_root = hamt.flush().unwrap();
    validate_tipset_lookup_hamt(&db, hamt_root, head).unwrap_err();
}
