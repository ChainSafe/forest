// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use actor::Multimap;
use address::Address;
use ipld_amt::Amt;

#[test]
fn basic_add() {
    let store = db::MemoryDB::default();
    let mut mm = Multimap::new(&store);

    let addr = Address::new_id(100).unwrap();
    assert_eq!(mm.get::<u64>(&addr.hash_key()), Ok(None));

    mm.add(addr.hash_key(), 8).unwrap();
    let arr: Amt<u64, _> = mm.get(&addr.hash_key()).unwrap().unwrap();
    assert_eq!(arr.get(0), Ok(Some(8)));

    mm.add(addr.hash_key(), 2).unwrap();
    mm.add(addr.hash_key(), 78).unwrap();
}

#[test]
fn for_each() {
    let store = db::MemoryDB::default();
    let mut mm = Multimap::new(&store);

    let addr = Address::new_id(100).unwrap();
    assert_eq!(mm.get::<u64>(&addr.hash_key()), Ok(None));

    mm.add(addr.hash_key(), 8).unwrap();
    mm.add(addr.hash_key(), 2).unwrap();
    mm.add(addr.hash_key(), 3).unwrap();
    mm.add("Some other string".to_owned(), 7).unwrap();

    let mut vals: Vec<(u64, u64)> = Vec::new();
    mm.for_each(&addr.hash_key(), |i, v| {
        vals.push((i, v));
        Ok(())
    })
    .unwrap();

    assert_eq!(&vals, &[(0, 8), (1, 2), (2, 3)])
}

#[test]
fn remove_all() {
    let store = db::MemoryDB::default();
    let mut mm = Multimap::new(&store);

    let addr1 = Address::new_id(100).unwrap();
    let addr2 = Address::new_id(101).unwrap();

    mm.add(addr1.hash_key(), 8).unwrap();
    mm.add(addr1.hash_key(), 88).unwrap();
    mm.add(addr2.hash_key(), 1).unwrap();

    let arr: Amt<u64, _> = mm.get(&addr1.hash_key()).unwrap().unwrap();
    assert_eq!(arr.get(1), Ok(Some(88)));

    mm.remove_all(addr1.hash_key()).unwrap();
    assert_eq!(mm.get::<u64>(&addr1.hash_key()), Ok(None));

    assert!(mm.get::<u64>(&addr2.hash_key()).unwrap().is_some());
    mm.remove_all(addr2.hash_key()).unwrap();
    assert_eq!(mm.get::<u64>(&addr2.hash_key()), Ok(None));
}
