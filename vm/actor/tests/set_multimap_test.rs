// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use actor::{u64_key, SetMultimap};
use address::Address;

#[test]
fn put_remove() {
    let store = db::MemoryDB::default();
    let mut smm = SetMultimap::new(&store);

    let addr = Address::new_id(100);
    assert_eq!(smm.get(&addr), Ok(None));

    smm.put(&addr, 8).unwrap();
    smm.put(&addr, 2).unwrap();
    smm.remove(&addr, 2).unwrap();

    let set = smm.get(&addr).unwrap().unwrap();
    assert_eq!(set.has(&u64_key(8)), Ok(true));
    assert_eq!(set.has(&u64_key(2)), Ok(false));

    smm.remove_all(&addr).unwrap();
    assert_eq!(smm.get(&addr), Ok(None));
}

#[test]
fn for_each() {
    let store = db::MemoryDB::default();
    let mut smm = SetMultimap::new(&store);

    let addr = Address::new_id(100);
    assert_eq!(smm.get(&addr), Ok(None));

    smm.put(&addr, 8).unwrap();
    smm.put(&addr, 3).unwrap();
    smm.put(&addr, 2).unwrap();
    smm.put(&addr, 8).unwrap();

    let mut vals: Vec<u64> = Vec::new();
    smm.for_each(&addr, |i| {
        vals.push(i);
        Ok(())
    })
    .unwrap();

    assert_eq!(vals.len(), 3);
}
