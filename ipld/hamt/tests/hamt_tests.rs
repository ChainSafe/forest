// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_encoding::{de::DeserializeOwned, ser::Serialize};
use ipld_blockstore::BlockStore;
use ipld_hamt::{Error, Hamt};
use std::fmt::Debug;

#[test]
fn test_basics() {
    let store = db::MemoryDB::default();
    let mut hamt = Hamt::new(&store);
    assert!(hamt.insert(1, "world".to_string()).is_none());

    assert_eq!(hamt.get(&1), Some(&"world".to_string()));
    assert_eq!(
        hamt.insert(1, "world2".to_string()),
        Some("world".to_string())
    );
    assert_eq!(hamt.get(&1), Some(&"world2".to_string()));
}

#[test]
fn test_from_link() {
    let store = db::MemoryDB::default();

    let mut hamt: Hamt<usize, String, _> = Hamt::new(&store);
    assert!(hamt.insert(1, "world".to_string()).is_none());

    assert_eq!(hamt.get(&1), Some(&"world".to_string()));
    assert_eq!(
        hamt.insert(1, "world2".to_string()),
        Some("world".to_string())
    );
    assert_eq!(hamt.get(&1), Some(&"world2".to_string()));
    let c = store.put(&hamt).unwrap();

    let new_hamt = Hamt::from_link(&c, &store).unwrap();
    assert_eq!(hamt, new_hamt);

    // insert value in the first one
    hamt.insert(2, "stuff".to_string());

    // loading original hash should returnnot be equal now
    let new_hamt = Hamt::from_link(&c, &store).unwrap();
    assert_ne!(hamt, new_hamt);

    // loading new hash
    let c2 = store.put(&hamt).unwrap();
    let new_hamt = Hamt::from_link(&c2, &store).unwrap();
    assert_eq!(hamt, new_hamt);

    // loading from an empty store does not work
    let empty_store = db::MemoryDB::default();
    assert!(Hamt::<usize, String, _>::from_link(&c2, &empty_store).is_err());

    // storing the hamt should produce the same cid as storing the root
    let c3 = store.put(&hamt).unwrap();
    assert_eq!(c3, c2);
}
