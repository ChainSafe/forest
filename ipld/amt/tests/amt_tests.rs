// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use encoding::{de::DeserializeOwned, ser::Serialize};
use ipld_amt::{Amt, Error, MAX_INDEX};
use ipld_blockstore::{BSStats, BlockStore, TrackingBlockStore};
use std::fmt::Debug;

fn assert_get<V, BS>(a: &Amt<V, BS>, i: u64, v: &V)
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

    a.set(2, "foo".to_owned()).unwrap();
    assert_get(&a, 2, &"foo".to_owned());
    assert_eq!(a.count(), 1);

    let c = a.flush().unwrap();

    let new_amt = Amt::load(&c, &db).unwrap();
    assert_get(&new_amt, 2, &"foo".to_owned());

    assert_eq!(
        c.to_string().as_str(),
        "bafy2bzacea4z4wxtdoo6ikgqkoe4gm364xhdtreycvfy5txvprpbtunx5jnwy"
    );
    assert_eq!(
        *db.stats.borrow(),
        BSStats {
            r: 1,
            w: 1,
            br: 12,
            bw: 12
        }
    );
}

#[test]
fn out_of_range() {
    let mem = db::MemoryDB::default();
    let db = TrackingBlockStore::new(&mem);
    let mut a = Amt::new(&db);

    let res = a.set((1 << 63) + 4, "what is up".to_owned());
    assert!(matches!(res, Err(Error::OutOfRange(_))));

    let res = a.set(MAX_INDEX + 1, "what is up".to_owned());
    assert!(matches!(res, Err(Error::OutOfRange(_))));

    let res = a.set(MAX_INDEX, "what is up".to_owned());
    assert_eq!(res.err(), None);
    // 20 is the max height, custom value to avoid exporting
    assert_eq!(a.height(), 20);

    let c = a.flush().unwrap();
    assert_eq!(
        c.to_string().as_str(),
        "bafy2bzacedozbiofn5fnrtfzy3tk5k7inyp5ncusdtb6xyl5z4rstdgqbad7g"
    );
    #[rustfmt::skip]
    assert_eq!(*db.stats.borrow(), BSStats {r: 0, w: 21, br: 0, bw: 979});
}

#[test]
fn expand() {
    let mem = db::MemoryDB::default();
    let db = TrackingBlockStore::new(&mem);
    let mut a = Amt::new(&db);

    a.set(2, "foo".to_owned()).unwrap();
    a.set(11, "bar".to_owned()).unwrap();
    a.set(79, "baz".to_owned()).unwrap();

    assert_get(&a, 2, &"foo".to_owned());
    assert_get(&a, 11, &"bar".to_owned());
    assert_get(&a, 79, &"baz".to_owned());

    // Flush and save root node and get cid
    let c = a.flush().unwrap();

    // Load amt with that cid
    let new_amt = Amt::load(&c, &db).unwrap();

    assert_get(&new_amt, 2, &"foo".to_owned());
    assert_get(&new_amt, 11, &"bar".to_owned());
    assert_get(&new_amt, 79, &"baz".to_owned());

    assert_eq!(
        c.to_string().as_str(),
        "bafy2bzaced25ah2r4gcerysjyrjqpqw72jvdy5ziwxk53ldxibktwmgkfgc22"
    );
    // TODO go implementation has a lot more writes, need to flush on expand to match if not changed
    #[rustfmt::skip]
    assert_eq!(*db.stats.borrow(), BSStats {r: 9, w: 6, br: 369, bw: 260});
}

#[test]
fn bulk_insert() {
    let mem = db::MemoryDB::default();
    let db = TrackingBlockStore::new(&mem);
    let mut a = Amt::new(&db);

    let iterations: u64 = 5000;

    for i in 0..iterations {
        a.set(i, "foo foo bar".to_owned()).unwrap();
    }

    for i in 0..iterations {
        assert_get(&a, i, &"foo foo bar".to_owned());
    }

    assert_eq!(a.count(), iterations);

    // Flush and regenerate amt
    let c = a.flush().unwrap();
    let new_amt = Amt::load(&c, &db).unwrap();

    for i in 0..iterations {
        assert_get(&new_amt, i, &"foo foo bar".to_owned());
    }

    assert_eq!(
        c.to_string().as_str(),
        "bafy2bzacedjhcq7542wu7ike4i4srgq7hwxxc5pmw5sub4secqk33mugl4zda"
    );
    // TODO go implementation has a lot more writes, need to flush on expand to match if not changed
    #[rustfmt::skip]
    assert_eq!(*db.stats.borrow(), BSStats {r: 1302, w: 717, br: 171567, bw: 94378});
}

#[test]
fn delete() {
    let mem = db::MemoryDB::default();
    let db = TrackingBlockStore::new(&mem);
    let mut a = Amt::new(&db);
    a.set(0, "ferret".to_owned()).unwrap();
    a.set(1, "ferret".to_owned()).unwrap();
    a.set(2, "ferret".to_owned()).unwrap();
    a.set(3, "ferret".to_owned()).unwrap();
    assert_eq!(a.count(), 4);

    a.delete(1).unwrap();
    assert!(a.get(1).unwrap().is_none());
    assert_eq!(a.count(), 3);

    assert_get(&a, 0, &"ferret".to_owned());
    assert_get(&a, 2, &"ferret".to_owned());
    assert_get(&a, 3, &"ferret".to_owned());

    a.delete(0).unwrap();
    a.delete(2).unwrap();
    a.delete(3).unwrap();
    assert_eq!(a.count(), 0);

    a.set(23, "dog".to_owned()).unwrap();
    a.set(24, "dog".to_owned()).unwrap();
    a.delete(23).unwrap();
    assert_eq!(a.count(), 1);
    assert_get(&a, 24, &"dog".to_owned());

    // Flush and regenerate amt
    let c = a.flush().unwrap();
    let regen_amt: Amt<String, _> = Amt::load(&c, &db).unwrap();
    assert_eq!(regen_amt.count(), 1);

    // Test that a new amt inserting just at index 24 is the same
    let mut new_amt = Amt::new(&db);
    new_amt.set(24, "dog".to_owned()).unwrap();
    let c2 = new_amt.flush().unwrap();

    assert_eq!(c, c2);
    assert_eq!(
        c.to_string().as_str(),
        "bafy2bzacedtq64mekzyjshwa3kxyt4kt5volln6acoavl4p7dexzczefwj7uw"
    );
    #[rustfmt::skip]
    assert_eq!(*db.stats.borrow(), BSStats {r: 1, w: 4, br: 51, bw: 120});
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
    assert_eq!(a.delete(10), Ok(false));
    assert_eq!(a.delete(0), Ok(false));
    assert_eq!(a.count(), 2);
    assert_eq!(a.get(1).unwrap(), Some(&"one".to_string()));
    assert_eq!(a.get(9).unwrap(), Some(&"nine".to_string()));
}

#[test]
fn delete_first_entry() {
    let mem = db::MemoryDB::default();
    let db = TrackingBlockStore::new(&mem);
    let mut a = Amt::new(&db);

    a.set(0, "cat".to_owned()).unwrap();
    a.set(27, "cat".to_owned()).unwrap();

    assert_eq!(a.count(), 2);
    assert_eq!(a.height(), 1);
    a.delete(27).unwrap();
    assert_eq!(a.count(), 1);
    assert_get(&a, 0, &"cat".to_owned());

    // Flush and regenerate amt
    let c = a.flush().unwrap();
    let new_amt: Amt<String, _> = Amt::load(&c, &db).unwrap();
    assert_eq!(new_amt.count(), 1);
    assert_eq!(new_amt.height(), 0);

    assert_eq!(
        c.to_string().as_str(),
        "bafy2bzacec4rpjwfkzp4n2wj233774cnhk5o2d7pub2gso2g3isdfxnxuhbr2"
    );
    #[rustfmt::skip]
    assert_eq!(*db.stats.borrow(), BSStats {r: 2, w: 2, br: 21, bw: 21});
}

#[test]
fn delete_reduce_height() {
    let mem = db::MemoryDB::default();
    let db = TrackingBlockStore::new(&mem);
    let mut a = Amt::new(&db);

    a.set(1, "thing".to_owned()).unwrap();
    let c1 = a.flush().unwrap();

    a.set(37, "other".to_owned()).unwrap();
    assert_eq!(a.height(), 1);
    let c2 = a.flush().unwrap();

    let mut a2: Amt<String, _> = Amt::load(&c2, &db).unwrap();
    a2.delete(37).unwrap();
    assert_eq!(a2.count(), 1);
    assert_eq!(a2.height(), 0);

    let c3 = a2.flush().unwrap();
    assert_eq!(c1, c3);

    assert_eq!(
        c1.to_string().as_str(),
        "bafy2bzaceccdkhc6fuhybskfl4hoydekua2su2vz45molwmy3ah36pnsmntvc"
    );
    #[rustfmt::skip]
    assert_eq!(*db.stats.borrow(), BSStats {r: 3, w: 5, br: 116, bw: 144});
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
        a.set(*i, "value".to_owned()).unwrap();
    }

    // Ensure all values were added into the amt
    for i in indexes.iter() {
        a.set(*i, "value".to_owned()).unwrap();
    }

    assert_eq!(a.count(), indexes.len() as u64);

    // Flush and regenerate amt
    let c = a.flush().unwrap();
    let new_amt = Amt::load(&c, &db).unwrap();
    assert_eq!(new_amt.count(), indexes.len() as u64);

    let mut x = 0;
    new_amt
        .for_each(|i, _: &String| {
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

    assert_eq!(
        c.to_string().as_str(),
        "bafy2bzaceccb5cgeysdu6ferucawc6twfedv5gc3iqgh2ko7o7e25r5ucpf4u"
    );
    // TODO go implementation has a lot more writes, need to flush on expand to match if not changed
    #[rustfmt::skip]
    assert_eq!(*db.stats.borrow(), BSStats {r: 2016, w: 2016, br: 124875, bw: 124875});
}

#[test]
fn delete_bug_test() {
    let mem = db::MemoryDB::default();
    let db = TrackingBlockStore::new(&mem);
    let mut a = Amt::new(&db);
    let empty_cid = a.flush().unwrap();

    let k = 100_000;

    a.set(k, "foo".to_owned()).unwrap();
    a.delete(k).unwrap();

    let c = a.flush().unwrap();

    // ! This is a bug, functionality needed to be locked in because this is what is expected
    // ! for the go implementation and could not be changed.
    assert_eq!(
        empty_cid.to_string().as_str(),
        "bafy2bzacedswlcz5ddgqnyo3sak3jmhmkxashisnlpq6ujgyhe4mlobzpnhs6"
    );
    assert_eq!(
        c.to_string().as_str(),
        "bafy2bzacec3ltjhtro3i4usbev24phgv6hb4fbfdaa2lxid4uod3zw4v3uce6"
    );
    #[rustfmt::skip]
    assert_eq!(*db.stats.borrow(), BSStats {r: 0, w: 2, br: 0, bw: 16});

    // * Testing bug functionality
    let mut new_amt = Amt::load(&c, &db).unwrap();
    new_amt.set(9, "foo".to_owned()).unwrap();
    assert_eq!(new_amt.get(9).unwrap(), Some(&"foo".to_string()));
    assert_eq!(new_amt.height(), 5);
    new_amt.set(66, "bar".to_owned()).unwrap();
    assert_eq!(new_amt.get(66).unwrap(), Some(&"bar".to_string()));
    new_amt.set(515, "baz".to_owned()).unwrap();
    assert_eq!(new_amt.get(515).unwrap(), Some(&"baz".to_string()));
    assert_eq!(new_amt.height(), 5);

    assert_eq!(new_amt.delete(9).unwrap(), true);
    assert_eq!(new_amt.height(), 3);
    assert_eq!(new_amt.delete(515).unwrap(), true);
    assert_eq!(new_amt.height(), 2);

    let c = new_amt.flush().unwrap();
    assert_eq!(
        c.to_string().as_str(),
        "bafy2bzaceblz37c42237c42h3y7vwzcqdjolm6fmmturwcifbd7zx2afvazke"
    );
    #[rustfmt::skip]
    assert_eq!(*db.stats.borrow(), BSStats {r: 1, w: 5, br: 8, bw: 124});
}
