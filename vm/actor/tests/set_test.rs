// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use actor::Set;

#[test]
fn put() {
    let store = db::MemoryDB::default();
    let mut set = Set::new(&store);

    let key = "test";
    assert_eq!(set.has(&key), Ok(false));

    set.put(key.to_owned()).unwrap();
    assert_eq!(set.has(&key), Ok(true));
}

#[test]
fn collect_keys() {
    let store = db::MemoryDB::default();
    let mut set = Set::new(&store);

    set.put("0".to_owned()).unwrap();

    assert_eq!(set.collect_keys().unwrap(), ["0".to_owned()]);

    set.put("1".to_owned()).unwrap();
    set.put("2".to_owned()).unwrap();
    set.put("3".to_owned()).unwrap();

    assert_eq!(set.collect_keys().unwrap().len(), 4);
}

#[test]
fn delete() {
    let store = db::MemoryDB::default();
    let mut set = Set::new(&store);

    assert_eq!(set.has(&"0"), Ok(false));
    set.put("0".to_owned()).unwrap();
    assert_eq!(set.has(&"0"), Ok(true));
    set.delete(&"0").unwrap();
    assert_eq!(set.has(&"0"), Ok(false));

    // Test delete when doesn't exist doesn't error
    set.delete(&"0").unwrap();
}
