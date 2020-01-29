// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use encoding::{ser::Serialize, to_vec};
use ipld_amt::*;

fn assert_get<S, DB>(a: &mut AMT<DB>, i: u64, v: &S)
where
    S: Serialize,
    DB: BlockStore,
{
    assert_eq!(a.get(i).unwrap().unwrap(), to_vec(&v).unwrap());
}

fn assert_count<DB>(a: &mut AMT<DB>, c: u64)
where
    DB: BlockStore,
{
    assert_eq!(a.count(), c);
}

#[test]
fn constructor() {
    AMT::new(&db::MemoryDB::default());
}

#[test]
fn basic_get_set() {
    let db = db::MemoryDB::default();
    let mut a = AMT::new(&db);

    a.set(2, &"foo").unwrap();
    assert_get(&mut a, 2, &"foo");
    assert_count(&mut a, 1);
}

#[test]
fn out_of_range() {
    let db = db::MemoryDB::default();
    let mut a = AMT::new(&db);

    let res = a.set(1 << 50, &"test");
    assert_eq!(res.err(), Some(Error::OutOfRange(1 << 50)));

    let res = a.set(MAX_INDEX, &"test");
    assert_eq!(res.err(), Some(Error::OutOfRange(MAX_INDEX)));

    let res = a.set(MAX_INDEX - 1, &"test");
    assert_eq!(res.err(), None);
    assert_get(&mut a, MAX_INDEX - 1, &"test");
}

#[test]
fn expand() {
    let db = db::MemoryDB::default();
    let mut a = AMT::new(&db);

    a.set(2, &"foo").unwrap();
    a.set(11, &"bar").unwrap();
    a.set(79, &"baz").unwrap();

    assert_get(&mut a, 2, &"foo");
    assert_get(&mut a, 11, &"bar");
    assert_get(&mut a, 79, &"baz");

    // Flush and save root node and get cid
    let c = a.flush().unwrap();

    // Load amt with that cid
    let mut new_amt = AMT::load(&db, &c).unwrap();

    assert_get(&mut new_amt, 2, &"foo");
    assert_get(&mut new_amt, 11, &"bar");
    assert_get(&mut new_amt, 79, &"baz");
}

#[test]
fn bulk_insert() {
    let db = db::MemoryDB::default();
    let mut a = AMT::new(&db);

    let iterations: u64 = 5000;

    for i in 0..iterations {
        a.set(i, &"foo foo bar").unwrap();
    }

    for i in 0..iterations {
        assert_get(&mut a, i, &"foo foo bar");
    }

    assert_eq!(a.count(), iterations);

    // Flush and regenerate amt
    let c = a.flush().unwrap();
    let mut new_amt = AMT::load(&db, &c).unwrap();

    for i in 0..iterations {
        assert_get(&mut new_amt, i, &"foo foo bar");
    }
}

#[test]
fn delete() {
    let db = db::MemoryDB::default();
    let mut a = AMT::new(&db);
    a.set(0, &"ferret").unwrap();
    a.set(1, &"ferret").unwrap();
    a.set(2, &"ferret").unwrap();
    a.set(3, &"ferret").unwrap();
    assert_eq!(a.count(), 4);

    a.delete(1).unwrap();
    assert!(a.get(1).unwrap().is_none());
    assert_eq!(a.count(), 3);

    assert_get(&mut a, 0, &"ferret");
    assert_get(&mut a, 2, &"ferret");
    assert_get(&mut a, 3, &"ferret");

    a.delete(0).unwrap();
    a.delete(2).unwrap();
    a.delete(3).unwrap();
    assert_eq!(a.count(), 0);

    a.set(23, &"dog").unwrap();
    a.set(24, &"dog").unwrap();
    a.delete(23).unwrap();
    assert_eq!(a.count(), 1);

    // Flush and regenerate amt
    let c = a.flush().unwrap();
    let regen_amt = AMT::load(&db, &c).unwrap();
    assert_eq!(regen_amt.count(), 1);

    // Test that a new amt inserting just at index 24 is the same
    let mut new_amt = AMT::new(&db);
    new_amt.set(24, &"dog").unwrap();
    let c2 = new_amt.flush().unwrap();

    assert_eq!(c, c2);
}

#[test]
fn delete_first_entry() {
    let db = db::MemoryDB::default();
    let mut a = AMT::new(&db);

    a.set(0, &"cat").unwrap();
    a.set(27, &"cat").unwrap();

    assert_eq!(a.count(), 2);
    a.delete(27).unwrap();
    assert_eq!(a.count(), 1);
    assert_get(&mut a, 0, &"cat");

    // Flush and regenerate amt
    let c = a.flush().unwrap();
    let new_amt = AMT::load(&db, &c).unwrap();
    assert_eq!(new_amt.count(), 1);
    assert_eq!(new_amt.height(), 0);
}

#[test]
fn delete_reduce_height() {
    let db = db::MemoryDB::default();
    let mut a = AMT::new(&db);

    a.set(0, &"thing").unwrap();
    let c1 = a.flush().unwrap();

    a.set(37, &"other").unwrap();
    assert_eq!(a.height(), 1);
    let c2 = a.flush().unwrap();

    let mut a2 = AMT::load(&db, &c2).unwrap();
    a2.delete(37).unwrap();
    assert_eq!(a2.count(), 1);
    assert_eq!(a2.height(), 0);

    let c3 = a2.flush().unwrap();
    assert_eq!(c1, c3);
}
