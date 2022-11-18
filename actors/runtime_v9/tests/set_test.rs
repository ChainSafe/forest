// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fil_actors_runtime_v9::Set;

#[test]
fn put() {
    let store = fvm_ipld_blockstore::MemoryBlockstore::new();
    let mut set = Set::new(&store);

    let key = "test".as_bytes();
    assert!(!set.has(key).unwrap());

    set.put(key.into()).unwrap();
    assert!(set.has(key).unwrap());
}

#[test]
fn collect_keys() {
    let store = fvm_ipld_blockstore::MemoryBlockstore::new();
    let mut set = Set::new(&store);

    set.put("0".into()).unwrap();

    assert_eq!(set.collect_keys().unwrap(), ["0".into()]);

    set.put("1".into()).unwrap();
    set.put("2".into()).unwrap();
    set.put("3".into()).unwrap();

    assert_eq!(set.collect_keys().unwrap().len(), 4);
}

#[test]
fn delete() {
    let store = fvm_ipld_blockstore::MemoryBlockstore::new();
    let mut set = Set::new(&store);

    let key = "0".as_bytes();

    assert!(!set.has(key).unwrap());
    set.put(key.into()).unwrap();
    assert!(set.has(key).unwrap());
    set.delete(key).unwrap();
    assert!(!set.has(key).unwrap());

    // Test delete when doesn't exist doesn't error
    set.delete(key).unwrap();
}
