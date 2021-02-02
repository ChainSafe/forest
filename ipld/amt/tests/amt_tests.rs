// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use encoding::{de::DeserializeOwned, ser::Serialize, BytesDe};
use ipld_amt::{Amt, Error, MAX_INDEX};
use ipld_blockstore::{BSStats, BlockStore, TrackingBlockStore};
use std::fmt::Debug;

fn assert_get<V, BS>(a: &Amt<V, BS>, i: usize, v: &V)
where
    V: Serialize + DeserializeOwned + PartialEq + Debug,
    BS: BlockStore,
{
    assert_eq!(a.get(i).unwrap().unwrap(), v);
}

#[test]
fn basic_get_set() {
    let mem = db::MemoryDB::default();
    let db = TrackingBlockStore::new(&mem);
    let mut a = Amt::new(&db);

    a.set(2, tbytes(b"foo")).unwrap();
    assert_get(&a, 2, &tbytes(b"foo"));
    assert_eq!(a.count(), 1);

    let c = a.flush().unwrap();

    let new_amt = Amt::load(&c, &db).unwrap();
    assert_get(&new_amt, 2, &tbytes(b"foo"));
    let c = a.flush().unwrap();

    assert_eq!(
        c.to_string().as_str(),
        "bafy2bzacedv5uu5za6oqtnozjvju5lhbgaybayzhw4txiojw7hd47ktgbv5wc"
    );
    #[rustfmt::skip]
    assert_eq!(*db.stats.borrow(), BSStats {r: 1, w: 2, br: 13, bw: 26});
}

#[test]
fn out_of_range() {
    let mem = db::MemoryDB::default();
    let db = TrackingBlockStore::new(&mem);
    let mut a = Amt::new(&db);

    let res = a.set(MAX_INDEX, tbytes(b"what is up"));
    assert!(res.err().is_none());
    // 21 is the max height, custom value to avoid exporting
    assert_eq!(a.height(), 21);

    let res = a.set(MAX_INDEX + 1, tbytes(b"what is up"));
    assert!(matches!(res, Err(Error::OutOfRange(_))));

    let res = a.set(MAX_INDEX - 1, tbytes(b"what is up"));
    assert!(res.err().is_none());
    assert_eq!(a.height(), 21);

    let c = a.flush().unwrap();
    assert_eq!(
        c.to_string().as_str(),
        "bafy2bzacecl3zuubhdvkojg6uhbu4mebaehx554q6algfjitqiivvnrqprkxo"
    );
    #[rustfmt::skip]
    assert_eq!(*db.stats.borrow(), BSStats {r: 0, w: 22, br: 0, bw: 1039});
}

#[test]
fn expand() {
    let mem = db::MemoryDB::default();
    let db = TrackingBlockStore::new(&mem);
    let mut a = Amt::new(&db);

    a.set(2, tbytes(b"foo")).unwrap();
    a.set(11, tbytes(b"bar")).unwrap();
    a.set(79, tbytes(b"baz")).unwrap();

    assert_get(&a, 2, &tbytes(b"foo"));
    assert_get(&a, 11, &tbytes(b"bar"));
    assert_get(&a, 79, &tbytes(b"baz"));

    // Flush and save root node and get cid
    let c = a.flush().unwrap();

    // Load amt with that cid
    let new_amt = Amt::load(&c, &db).unwrap();

    assert_get(&new_amt, 2, &tbytes(b"foo"));
    assert_get(&new_amt, 11, &tbytes(b"bar"));
    assert_get(&new_amt, 79, &tbytes(b"baz"));

    assert_eq!(
        c.to_string().as_str(),
        "bafy2bzacecughjbclx3lbqwibrwc6pe7nttlc3qewedsrayghsvh5j5lpofiq"
    );

    #[rustfmt::skip]
    assert_eq!(*db.stats.borrow(), BSStats {r: 6, w: 6, br: 261, bw: 261});
}

#[test]
fn bulk_insert() {
    let mem = db::MemoryDB::default();
    let db = TrackingBlockStore::new(&mem);
    let mut a = Amt::new(&db);

    let iterations: usize = 5000;

    for i in 0..iterations {
        a.set(i, tbytes(b"foo foo bar")).unwrap();
    }

    for i in 0..iterations {
        assert_get(&a, i, &tbytes(b"foo foo bar"));
    }

    assert_eq!(a.count(), iterations);

    // Flush and regenerate amt
    let c = a.flush().unwrap();
    let new_amt = Amt::load(&c, &db).unwrap();

    for i in 0..iterations {
        assert_get(&new_amt, i, &tbytes(b"foo foo bar"));
    }

    assert_eq!(
        c.to_string().as_str(),
        "bafy2bzacecfquuqzqzlox25aynodzw2qhxijdzfvno6tibyes3kb6nd3f7uxa"
    );

    #[rustfmt::skip]
    assert_eq!(*db.stats.borrow(), BSStats {r: 717, w: 717, br: 94379, bw: 94379});
}

#[test]
fn flush_read() {
    let mem = db::MemoryDB::default();
    let db = TrackingBlockStore::new(&mem);
    let mut a = Amt::new(&db);

    let iterations: usize = 100;

    for i in 0..iterations {
        a.set(i, tbytes(b"foo foo bar")).unwrap();
    }

    for i in 0..iterations {
        assert_get(&a, i, &tbytes(b"foo foo bar"));
    }

    // Flush but don't reload from Cid
    a.flush().unwrap();

    // These reads can hit cache, if saved
    for i in 0..iterations {
        assert_get(&a, i, &tbytes(b"foo foo bar"));
    }

    #[rustfmt::skip]
    assert_eq!(*db.stats.borrow(), BSStats {r: 0, w: 16, br: 0, bw: 1930});
}

#[test]
fn delete() {
    let mem = db::MemoryDB::default();
    let db = TrackingBlockStore::new(&mem);
    let mut a = Amt::new(&db);
    a.set(0, tbytes(b"cat")).unwrap();
    a.set(1, tbytes(b"cat")).unwrap();
    a.set(2, tbytes(b"cat")).unwrap();
    a.set(3, tbytes(b"cat")).unwrap();
    assert_eq!(a.count(), 4);

    a.delete(1).unwrap();
    assert!(a.get(1).unwrap().is_none());
    assert_eq!(a.count(), 3);

    assert_get(&a, 0, &tbytes(b"cat"));
    assert_get(&a, 2, &tbytes(b"cat"));
    assert_get(&a, 3, &tbytes(b"cat"));

    a.delete(0).unwrap();
    a.delete(2).unwrap();
    a.delete(3).unwrap();
    assert_eq!(a.count(), 0);

    a.set(23, tbytes(b"dog")).unwrap();
    a.set(24, tbytes(b"dog")).unwrap();
    a.delete(23).unwrap();
    assert_eq!(a.count(), 1);

    // Flush and regenerate amt
    let c = a.flush().unwrap();
    let regen_amt: Amt<BytesDe, _> = Amt::load(&c, &db).unwrap();
    assert_eq!(regen_amt.count(), 1);

    // Test that a new amt inserting just at index 24 is the same
    let mut new_amt = Amt::new(&db);
    new_amt.set(24, tbytes(b"dog")).unwrap();
    let c2 = new_amt.flush().unwrap();

    assert_eq!(c, c2);
    assert_eq!(
        c.to_string().as_str(),
        "bafy2bzacebnnxpurpb3zqqr22i7ch4uruz6hgykn3ryzoo4hh3ox2m2kufsvg"
    );
    #[rustfmt::skip]
    assert_eq!(*db.stats.borrow(), BSStats {r: 1, w: 4, br: 52, bw: 122});
}

#[test]
fn delete_fail_check() {
    let mem = db::MemoryDB::default();
    let db = TrackingBlockStore::new(&mem);
    let mut a = Amt::new(&db);

    a.set(1, "one".to_owned()).unwrap();
    a.set(9, "nine".to_owned()).unwrap();
    assert_eq!(a.height(), 1);
    assert_eq!(a.count(), 2);
    assert_eq!(a.get(1).unwrap(), Some(&"one".to_string()));
    assert_eq!(a.get(9).unwrap(), Some(&"nine".to_string()));
    assert!(a.delete(10).unwrap().is_none());
    assert!(a.delete(0).unwrap().is_none());
    assert_eq!(a.count(), 2);
    assert_eq!(a.get(1).unwrap(), Some(&"one".to_string()));
    assert_eq!(a.get(9).unwrap(), Some(&"nine".to_string()));
}

#[test]
fn delete_first_entry() {
    let mem = db::MemoryDB::default();
    let db = TrackingBlockStore::new(&mem);
    let mut a = Amt::new(&db);

    a.set(0, tbytes(b"cat")).unwrap();
    a.set(27, tbytes(b"cat")).unwrap();

    assert_eq!(a.count(), 2);
    assert_eq!(a.height(), 1);
    a.delete(27).unwrap();
    assert_eq!(a.count(), 1);
    assert_get(&a, 0, &tbytes(b"cat"));

    // Flush and regenerate amt
    let c = a.flush().unwrap();
    let new_amt: Amt<BytesDe, _> = Amt::load(&c, &db).unwrap();
    assert_eq!(new_amt.count(), 1);
    assert_eq!(new_amt.height(), 0);

    assert_eq!(
        c.to_string().as_str(),
        "bafy2bzacecmxrjeri2ojuy3riae2mpbnztx2hqggqgkcrxao2nwpga77j2vqe"
    );

    #[rustfmt::skip]
    assert_eq!(*db.stats.borrow(), BSStats {r: 1, w: 1, br: 13, bw: 13});
}

#[test]
fn delete_reduce_height() {
    let mem = db::MemoryDB::default();
    let db = TrackingBlockStore::new(&mem);
    let mut a = Amt::new(&db);

    a.set(1, tbytes(b"thing")).unwrap();
    let c1 = a.flush().unwrap();

    a.set(37, tbytes(b"other")).unwrap();
    assert_eq!(a.height(), 1);
    let c2 = a.flush().unwrap();

    let mut a2: Amt<BytesDe, _> = Amt::load(&c2, &db).unwrap();
    assert_eq!(a2.count(), 2);
    assert_eq!(a2.height(), 1);
    assert!(a2.delete(37).unwrap().is_some());
    assert_eq!(a2.count(), 1);
    assert_eq!(a2.height(), 0);

    let c3 = a2.flush().unwrap();
    assert_eq!(c1, c3);

    assert_eq!(
        c1.to_string().as_str(),
        "bafy2bzacebmkyah6kppbszluix3g332hntzx6wfdcqcr5hjdsaxri5jhgrdmo"
    );
    #[rustfmt::skip]
    assert_eq!(*db.stats.borrow(), BSStats {r: 3, w: 5, br: 117, bw: 147});
}

#[test]
fn for_each() {
    let mem = db::MemoryDB::default();
    let db = TrackingBlockStore::new(&mem);
    let mut a = Amt::new(&db);

    let mut indexes = Vec::new();
    for i in 0..10000 {
        if (i + 1) % 3 == 0 {
            indexes.push(i);
        }
    }

    // Set all indices in the Amt
    for i in indexes.iter() {
        a.set(*i, tbytes(b"value")).unwrap();
    }

    // Ensure all values were added into the amt
    for i in indexes.iter() {
        assert_eq!(a.get(*i).unwrap(), Some(&tbytes(b"value")));
    }

    assert_eq!(a.count(), indexes.len() as usize);

    // Iterate over amt with dirty cache
    let mut x = 0;
    a.for_each(|_, _: &BytesDe| {
        x += 1;
        Ok(())
    })
    .unwrap();

    assert_eq!(x, indexes.len());

    // Flush and regenerate amt
    let c = a.flush().unwrap();
    let new_amt = Amt::load(&c, &db).unwrap();
    assert_eq!(new_amt.count(), indexes.len() as usize);

    let mut x = 0;
    new_amt
        .for_each(|i, _: &BytesDe| {
            if i != indexes[x] {
                panic!(
                    "for each found wrong index: expected {} got {}",
                    indexes[x], i
                );
            }
            x += 1;
            Ok(())
        })
        .unwrap();
    assert_eq!(x, indexes.len());

    // Iteration again will be read diff with go-interop, since they do not cache
    new_amt.for_each(|_, _: &BytesDe| Ok(())).unwrap();

    assert_eq!(
        c.to_string().as_str(),
        "bafy2bzaceanqxtbsuyhqgxubiq6vshtbhktmzp2if4g6kxzttxmzkdxmtipcm"
    );

    #[rustfmt::skip]
    assert_eq!(*db.stats.borrow(), BSStats {r: 1431, w: 1431, br: 88649, bw: 88649});
}

#[test]
fn for_each_mutate() {
    let mem = db::MemoryDB::default();
    let db = TrackingBlockStore::new(&mem);
    let mut a = Amt::new(&db);

    let indexes = [1, 9, 66, 74, 82, 515];

    // Set all indices in the Amt
    for &i in indexes.iter() {
        a.set(i, tbytes(b"value")).unwrap();
    }

    // Flush and regenerate amt
    let c = a.flush().unwrap();
    drop(a);
    let mut new_amt = Amt::load(&c, &db).unwrap();
    assert_eq!(new_amt.count(), indexes.len() as usize);

    new_amt
        .for_each_mut(|i, v: &mut ipld_amt::ValueMut<'_, BytesDe>| {
            if let 1 | 74 = i {
                // Value it's set to doesn't matter, just cloning for expedience
                **v = v.clone();
            }
            Ok(())
        })
        .unwrap();

    assert_eq!(
        c.to_string().as_str(),
        "bafy2bzaced44wtasbcukqjqicvxzcyn5up6sorr5khzbdkl6zjeo736f377ew"
    );

    #[rustfmt::skip]
    assert_eq!(*db.stats.borrow(), BSStats {r: 12, w: 12, br: 573, bw: 573});
}

#[test]
fn delete_bug_test() {
    let mem = db::MemoryDB::default();
    let db = TrackingBlockStore::new(&mem);
    let mut a = Amt::new(&db);
    let empty_cid = a.flush().unwrap();

    let k = 100_000;

    a.set(k, tbytes(b"foo")).unwrap();
    a.delete(k).unwrap();

    let c = a.flush().unwrap();

    assert_eq!(
        empty_cid.to_string().as_str(),
        "bafy2bzacedijw74yui7otvo63nfl3hdq2vdzuy7wx2tnptwed6zml4vvz7wee"
    );
    assert_eq!(c, empty_cid);
    #[rustfmt::skip]
    assert_eq!(*db.stats.borrow(), BSStats {r:0, w:2, br:0, bw:18});
}

fn tbytes(bz: &[u8]) -> BytesDe {
    BytesDe(bz.to_vec())
}
