// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::chain::index::tests::{genesis_tipset, persist_tipset, tipset_child};
use rstest::rstest;

enum HamtEntries {
    Checkpoints,
    All,
    Empty,
}

#[rstest]
#[case::success(HamtEntries::Checkpoints, None, None)]
#[case::non_checkpoint_entry(HamtEntries::All, None, Some("non checkpoint entries"))]
#[case::missing_checkpoint_entry(HamtEntries::Empty, None, Some("checkpoint epoch 40 with key"))]
#[case::bad_checkpoint_entry(
    HamtEntries::Checkpoints,
    Some(40),
    Some("tipset mismatch, checkpoint epoch: 40")
)]
#[case::null_checkpoint_entry(
    HamtEntries::Checkpoints,
    Some(20),
    Some("tipset mismatch, checkpoint epoch: 20")
)]
fn test_validate_tipset_lookup_hamt(
    #[case] entries: HamtEntries,
    #[case] genesis_at: Option<ChainEpoch>,
    #[case] expected_err: Option<&str>,
) {
    let db = Arc::new(MemoryDB::default());
    let mut hamt: Hamt<_, TipsetKey, ChainEpoch> =
        Hamt::new_with_bit_width(db.shallow_clone(), TIPSET_LOOKUP_HAMT_BIT_WIDTH);
    let genesis = genesis_tipset();
    persist_tipset(&genesis, &db);
    // Epoch 20 is a null round: 19 is followed directly by 21.
    let mut prev = genesis.shallow_clone();
    for epoch in [10, 19, 21, 30, 40, 41] {
        let ts = tipset_child(&prev, epoch);
        let set = match entries {
            HamtEntries::Checkpoints => ChainIndex::is_tipset_lookup_checkpoint(epoch),
            HamtEntries::All => true,
            HamtEntries::Empty => false,
        };
        if set {
            hamt.set(epoch, ts.key().clone()).unwrap();
        }
        persist_tipset(&ts, &db);
        prev = ts;
    }
    if let Some(epoch) = genesis_at {
        hamt.set(epoch, genesis.key().clone()).unwrap();
    }
    let hamt_root = hamt.flush().unwrap();
    let result = validate_tipset_lookup_hamt(&db, hamt_root, prev);
    match expected_err {
        None => result.unwrap(),
        Some(expected) => {
            let err = format!("{:#}", result.unwrap_err());
            assert!(err.contains(expected), "{err}");
        }
    }
}
