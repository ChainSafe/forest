// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
// Copyright 2021-2023 Protocol Labs
// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::fmt::Debug;

use crate::ipld_amt::{Amt, Amtv0, Error, MAX_INDEX};
use fvm_ipld_blockstore::tracking::{BSStats, TrackingBlockstore};
use fvm_ipld_blockstore::{Blockstore, MemoryBlockstore};
use fvm_ipld_encoding::de::DeserializeOwned;
use fvm_ipld_encoding::ser::Serialize;
use fvm_ipld_encoding::BytesDe;

fn assert_get<V, BS>(a: &Amt<V, BS>, i: u64, v: &V)
where
    V: Serialize + DeserializeOwned + PartialEq + Debug,
    BS: Blockstore,
{
    assert_eq!(a.get(i).unwrap().unwrap(), v);
}

fn assert_v0_get<V, BS>(a: &Amtv0<V, BS>, i: u64, v: &V)
where
    V: Serialize + DeserializeOwned + PartialEq + Debug,
    BS: Blockstore,
{
    assert_eq!(a.get(i).unwrap().unwrap(), v);
}

#[test]
fn basic_get_set() {
    let mem = MemoryBlockstore::default();
    let db = TrackingBlockstore::new(&mem);
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
    assert_eq!(*db.stats.borrow(), BSStats {r: 1, w: 1, br: 13, bw: 13});
}

#[test]
fn legacy_amtv0_basic_get_set() {
    let mem = MemoryBlockstore::default();
    let db = TrackingBlockstore::new(&mem);
    let mut a = Amtv0::new(&db);

    a.set(2, tbytes(b"foo")).unwrap();
    assert_v0_get(&a, 2, &tbytes(b"foo"));
    assert_eq!(a.count(), 1);

    let c = a.flush().unwrap();

    let new_amt = Amtv0::load(&c, &db).unwrap();
    assert_v0_get(&new_amt, 2, &tbytes(b"foo"));
    let c = a.flush().unwrap();

    assert_eq!(
        c.to_string().as_str(),
        "bafy2bzaceansvim5z2rzifilsbzsjuoul2adx7iad7x3b4paj3qsexqf6ovxk"
    );
    #[rustfmt::skip]
    assert_eq!(*db.stats.borrow(), BSStats {r: 1, w: 1, br: 12, bw: 12});
}

#[test]
fn out_of_range() {
    let mem = MemoryBlockstore::default();
    let db = TrackingBlockstore::new(&mem);
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
    let mem = MemoryBlockstore::default();
    let db = TrackingBlockstore::new(&mem);
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
    let mem = MemoryBlockstore::default();
    let db = TrackingBlockstore::new(&mem);
    let mut a = Amt::new(&db);

    let iterations: u64 = 5000;

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
    let mem = MemoryBlockstore::default();
    let db = TrackingBlockstore::new(&mem);
    let mut a = Amt::new(&db);

    let iterations: u64 = 100;

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
    let mem = fvm_ipld_blockstore::MemoryBlockstore::default();
    let db = TrackingBlockstore::new(&mem);
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
    let mem = fvm_ipld_blockstore::MemoryBlockstore::default();
    let db = TrackingBlockstore::new(&mem);
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
    let mem = fvm_ipld_blockstore::MemoryBlockstore::default();
    let db = TrackingBlockstore::new(&mem);
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
    let mem = MemoryBlockstore::default();
    let db = TrackingBlockstore::new(&mem);
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
    let mem = MemoryBlockstore::default();
    let db = TrackingBlockstore::new(&mem);
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

    assert_eq!(a.count(), indexes.len() as u64);

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
    assert_eq!(new_amt.count(), indexes.len() as u64);

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
fn for_each_ranged() {
    let mem = MemoryBlockstore::default();
    let db = TrackingBlockstore::new(&mem);
    let mut a = Amt::new(&db);

    let mut indexes = Vec::new();
    const RANGE: u64 = 1000;
    for i in 0..RANGE {
        indexes.push(i);
    }

    // Set all indices in the Amt
    for i in indexes.iter() {
        a.set(*i, tbytes(b"value")).unwrap();
    }

    // Ensure all values were added into the amt
    for i in indexes.iter() {
        assert_eq!(a.get(*i).unwrap(), Some(&tbytes(b"value")));
    }

    assert_eq!(a.count(), indexes.len() as u64);

    // Iterate over amt with dirty cache from different starting values
    for start_val in 0..RANGE {
        let mut retrieved_values = Vec::new();
        let (count, next_key) = a
            .for_each_while_ranged(Some(start_val), None, |index, _: &BytesDe| {
                retrieved_values.push(index);
                Ok(true)
            })
            .unwrap();

        // With no max set, next key should be None
        assert_eq!(next_key, None);
        assert_eq!(retrieved_values, indexes[start_val as usize..]);
        assert_eq!(count, retrieved_values.len() as u64);
    }

    // Iterate over amt with dirty cache with different page sizes
    for page_size in 1..=RANGE {
        let mut retrieved_values = Vec::new();
        let (count, next_key) = a
            .for_each_while_ranged(None, Some(page_size), |index, _: &BytesDe| {
                retrieved_values.push(index);
                Ok(true)
            })
            .unwrap();

        assert_eq!(retrieved_values, indexes[..page_size as usize]);
        assert_eq!(count, retrieved_values.len() as u64);
        if page_size == RANGE {
            assert_eq!(next_key, None);
        } else {
            assert_eq!(next_key, Some(page_size));
        }
    }

    // Chain requests over amt with dirty cache, request all items in pages of 100
    let page_size = 100;
    let mut retrieved_values = Vec::new();
    let mut start_cursor = None;
    loop {
        let (num_traversed, next_cursor) = a
            .for_each_while_ranged(start_cursor, Some(page_size), |idx, _val| {
                retrieved_values.push(idx);
                Ok(true)
            })
            .unwrap();

        assert_eq!(num_traversed, page_size);

        start_cursor = next_cursor;
        if start_cursor.is_none() {
            break;
        }
    }
    assert_eq!(retrieved_values, indexes);

    // Flush the AMT and reload it from the blockstore
    let c = a.flush().unwrap();
    let mut a = Amt::load(&c, &db).unwrap();
    assert_eq!(a.count(), indexes.len() as u64);

    let page_size = 100;
    let mut retrieved_values = Vec::new();
    let mut start_cursor = None;
    loop {
        let (num_traversed, next_cursor) = a
            .for_each_ranged(start_cursor, Some(page_size), |idx, _val: &BytesDe| {
                retrieved_values.push(idx);
                Ok(())
            })
            .unwrap();

        assert_eq!(num_traversed, page_size);

        start_cursor = next_cursor;
        if start_cursor.is_none() {
            break;
        }
    }
    assert_eq!(retrieved_values, indexes);

    // Now delete alternating blocks of 10 values from the AMT
    for i in 0..RANGE {
        if (i / 10) % 2 == 0 {
            a.delete(i).unwrap();
        }
    }

    // Iterate over the amt with dirty cache ignoring gaps in the address space including at the
    // beginning of the amt, we should only see the values that were not deleted
    let (num_traversed, next_cursor) = a
        .for_each_while_ranged(Some(0), Some(501), |i, _v| {
            assert_eq!((i / 10) % 2, 1); // only "odd" batches of ten 10 - 19, 30 - 39, etc. should be present
            Ok(true)
        })
        .unwrap();
    assert_eq!(num_traversed, 500); // only 500 values should still be traversed
    assert_eq!(next_cursor, None); // no next cursor should be returned

    // flush the amt to the blockstore, reload and repeat the test with a clean cache
    let cid = a.flush().unwrap();
    let a = Amt::load(&cid, &db).unwrap();
    let (num_traversed, next_cursor) = a
        .for_each_while_ranged(Some(0), Some(501), |i, _v: &BytesDe| {
            assert_eq!((i / 10) % 2, 1); // only "odd" batches of ten 10 - 19, 30 - 39, etc. should be present
            Ok(true)
        })
        .unwrap();
    assert_eq!(num_traversed, 500); // only 500 values should still be traversed
    assert_eq!(next_cursor, None); // no next cursor should be returned

    #[rustfmt::skip]

    assert_eq!(
        *db.stats.borrow(),
        BSStats {
            r: 263,
            w: 238,
            br: 21550,
            bw: 20225
        }
    );
}

#[test]
fn for_each_mutate() {
    let mem = MemoryBlockstore::default();
    let db = TrackingBlockstore::new(&mem);
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
    assert_eq!(new_amt.count(), indexes.len() as u64);

    new_amt
        .for_each_mut(|i, v: &mut crate::ipld_amt::ValueMut<'_, BytesDe>| {
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
    let mem = MemoryBlockstore::default();
    let db = TrackingBlockstore::new(&mem);
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

#[test]
fn new_from_iter() {
    let mem = MemoryBlockstore::default();
    let data: Vec<String> = (0..1000).map(|i| format!("thing{i}")).collect();
    let k = Amt::<&str, _>::new_from_iter(&mem, data.iter().map(|s| &**s)).unwrap();

    let a: Amt<String, _> = Amt::load(&k, &mem).unwrap();
    let mut restored = Vec::new();
    a.for_each(|k, v| {
        restored.push((k as usize, v.clone()));
        Ok(())
    })
    .unwrap();
    let expected: Vec<_> = data.into_iter().enumerate().collect();
    assert_eq!(expected, restored);
}
