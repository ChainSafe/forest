// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use clock::ChainEpoch;
use forest_actor::{u64_key, SetMultimap};

#[test]
fn put_remove() {
    let store = db::MemoryDB::default();
    let mut smm = SetMultimap::new(&store);

    let epoch: ChainEpoch = 100;
    assert_eq!(smm.get(epoch).unwrap(), None);

    smm.put(epoch, 8).unwrap();
    smm.put(epoch, 2).unwrap();
    smm.remove(epoch, 2).unwrap();

    let set = smm.get(epoch).unwrap().unwrap();
    assert_eq!(set.has(&u64_key(8)).unwrap(), true);
    assert_eq!(set.has(&u64_key(2)).unwrap(), false);

    smm.remove_all(epoch).unwrap();
    assert_eq!(smm.get(epoch).unwrap(), None);
}

#[test]
fn for_each() {
    let store = db::MemoryDB::default();
    let mut smm = SetMultimap::new(&store);

    let epoch: ChainEpoch = 100;
    assert_eq!(smm.get(epoch).unwrap(), None);

    smm.put(epoch, 8).unwrap();
    smm.put(epoch, 3).unwrap();
    smm.put(epoch, 2).unwrap();
    smm.put(epoch, 8).unwrap();

    let mut vals: Vec<u64> = Vec::new();
    smm.for_each(epoch, |i| {
        vals.push(i);
        Ok(())
    })
    .unwrap();

    assert_eq!(vals.len(), 3);
}
