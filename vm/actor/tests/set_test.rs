// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_actor::Set;

#[test]
fn put() {
    let store = db::MemoryDB::default();
    let mut set = Set::new(&store);

    let key = "test".as_bytes();
    assert_eq!(set.has(&key).unwrap(), false);

    set.put(key.into()).unwrap();
    assert_eq!(set.has(&key).unwrap(), true);
}

#[test]
fn collect_keys() {
    let store = db::MemoryDB::default();
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
    let store = db::MemoryDB::default();
    let mut set = Set::new(&store);

    let key = "0".as_bytes();

    assert_eq!(set.has(key).unwrap(), false);
    set.put(key.into()).unwrap();
    assert_eq!(set.has(key).unwrap(), true);
    set.delete(key).unwrap();
    assert_eq!(set.has(key).unwrap(), false);

    // Test delete when doesn't exist doesn't error
    set.delete(key).unwrap();
}
