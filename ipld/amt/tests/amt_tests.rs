// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use blockstore::BlockStore;
use encoding::{de::DeserializeOwned, ser::Serialize};
use ipld_amt::{Error, AMT, MAX_INDEX};
use std::fmt::Debug;

fn assert_get<DB, V>(a: &mut AMT<DB, V>, i: u64, v: &V)
where
    V: Clone + Serialize + DeserializeOwned + PartialEq + Debug,
    DB: BlockStore,
{
    assert_eq!(&a.get(i).unwrap().unwrap(), v);
}

fn assert_count<DB, V>(a: &mut AMT<DB, V>, c: u64)
where
    DB: BlockStore,
    V: Clone + Serialize + DeserializeOwned + PartialEq,
{
    assert_eq!(a.count(), c);
}

#[test]
fn constructor() {
    AMT::<_, u8>::new(&db::MemoryDB::default());
}

#[test]
fn basic_get_set() {
    let db = db::MemoryDB::default();
    let mut a = AMT::new(&db);

    a.set(2, "foo".to_owned()).unwrap();
    assert_get(&mut a, 2, &"foo".to_owned());
    assert_count(&mut a, 1);

    let c = a.flush().unwrap();
    assert_eq!(
        c.to_bytes(),
        hex::decode("0171a0e40220399e5af31b9de428d05389c3337ee5ce39c498154b8ecef57c5e19d1b7ea5b6c")
            .unwrap()
    );
}

#[test]
fn out_of_range() {
    let db = db::MemoryDB::default();
    let mut a = AMT::new(&db);

    let res = a.set(1 << 50, "test".to_owned());
    assert_eq!(res.err(), Some(Error::OutOfRange(1 << 50)));

    let res = a.set(MAX_INDEX, "test".to_owned());
    assert_eq!(res.err(), Some(Error::OutOfRange(MAX_INDEX)));

    let res = a.set(MAX_INDEX - 1, "test".to_owned());
    assert_eq!(res.err(), None);
    assert_get(&mut a, MAX_INDEX - 1, &"test".to_owned());
}

#[test]
fn expand() {
    let db = db::MemoryDB::default();
    let mut a = AMT::new(&db);

    a.set(2, "foo".to_owned()).unwrap();
    a.set(11, "bar".to_owned()).unwrap();
    a.set(79, "baz".to_owned()).unwrap();

    assert_get(&mut a, 2, &"foo".to_owned());
    assert_get(&mut a, 11, &"bar".to_owned());
    assert_get(&mut a, 79, &"baz".to_owned());

    // Flush and save root node and get cid
    let c = a.flush().unwrap();

    // Load amt with that cid
    let mut new_amt = AMT::load(&db, &c).unwrap();

    assert_get(&mut new_amt, 2, &"foo".to_owned());
    assert_get(&mut new_amt, 11, &"bar".to_owned());
    assert_get(&mut new_amt, 79, &"baz".to_owned());

    assert_eq!(
        c.to_bytes(),
        hex::decode("0171a0e40220f5d01f51e18448e249c45307c2dfd26a3c7728b5d5ddac7740553b30ca2985ad")
            .unwrap()
    );
}

#[test]
fn bulk_insert() {
    let db = db::MemoryDB::default();
    let mut a = AMT::new(&db);

    let iterations: u64 = 5000;

    for i in 0..iterations {
        a.set(i, "foo foo bar".to_owned()).unwrap();
    }

    for i in 0..iterations {
        assert_get(&mut a, i, &"foo foo bar".to_owned());
    }

    assert_eq!(a.count(), iterations);

    // Flush and regenerate amt
    let c = a.flush().unwrap();
    let mut new_amt = AMT::load(&db, &c).unwrap();

    for i in 0..iterations {
        assert_get(&mut new_amt, i, &"foo foo bar".to_owned());
    }

    assert_eq!(
        c.to_bytes(),
        hex::decode("0171a0e40220d27143fde6ad4fa144e239289a1f3daf7175ecb76540f2441415bdb2865f3230")
            .unwrap()
    );
}

#[test]
fn delete() {
    let db = db::MemoryDB::default();
    let mut a = AMT::new(&db);
    a.set(0, "ferret".to_owned()).unwrap();
    a.set(1, "ferret".to_owned()).unwrap();
    a.set(2, "ferret".to_owned()).unwrap();
    a.set(3, "ferret".to_owned()).unwrap();
    assert_eq!(a.count(), 4);

    a.delete(1).unwrap();
    assert!(a.get(1).unwrap().is_none());
    assert_eq!(a.count(), 3);

    assert_get(&mut a, 0, &"ferret".to_owned());
    assert_get(&mut a, 2, &"ferret".to_owned());
    assert_get(&mut a, 3, &"ferret".to_owned());

    a.delete(0).unwrap();
    a.delete(2).unwrap();
    a.delete(3).unwrap();
    assert_eq!(a.count(), 0);

    a.set(23, "dog".to_owned()).unwrap();
    a.set(24, "dog".to_owned()).unwrap();
    a.delete(23).unwrap();
    assert_eq!(a.count(), 1);

    // Flush and regenerate amt
    let c = a.flush().unwrap();
    let regen_amt: AMT<_, String> = AMT::load(&db, &c).unwrap();
    assert_eq!(regen_amt.count(), 1);

    // Test that a new amt inserting just at index 24 is the same
    let mut new_amt = AMT::new(&db);
    new_amt.set(24, "dog".to_owned()).unwrap();
    let c2 = new_amt.flush().unwrap();

    assert_eq!(c, c2);
    assert_eq!(
        c.to_bytes(),
        hex::decode("0171a0e40220e70f71845670991ec0daaf89f153ed5cb5b7c0138155f1ff192f916485b27f4b")
            .unwrap()
    );
}

#[test]
fn delete_first_entry() {
    let db = db::MemoryDB::default();
    let mut a = AMT::new(&db);

    a.set(0, "cat".to_owned()).unwrap();
    a.set(27, "cat".to_owned()).unwrap();

    assert_eq!(a.count(), 2);
    a.delete(27).unwrap();
    assert_eq!(a.count(), 1);
    assert_get(&mut a, 0, &"cat".to_owned());

    // Flush and regenerate amt
    let c = a.flush().unwrap();
    let new_amt: AMT<_, String> = AMT::load(&db, &c).unwrap();
    assert_eq!(new_amt.count(), 1);
    assert_eq!(new_amt.height(), 0);

    assert_eq!(
        c.to_bytes(),
        hex::decode("0171a0e40220b917a6c5565fc6eac9d6f7fff04d3abaed0fefa074693b46da2432ddb7a1c31d")
            .unwrap()
    );
}

#[test]
fn delete_reduce_height() {
    let db = db::MemoryDB::default();
    let mut a = AMT::new(&db);

    a.set(1, "thing".to_owned()).unwrap();
    let c1 = a.flush().unwrap();

    a.set(37, "other".to_owned()).unwrap();
    assert_eq!(a.height(), 1);
    let c2 = a.flush().unwrap();

    let mut a2: AMT<_, String> = AMT::load(&db, &c2).unwrap();
    a2.delete(37).unwrap();
    assert_eq!(a2.count(), 1);
    assert_eq!(a2.height(), 0);

    let c3 = a2.flush().unwrap();
    assert_eq!(c1, c3);

    assert_eq!(
        c1.to_bytes(),
        hex::decode("0171a0e4022084351c5e2d0f80c9455f0eec0c8aa0352a6ab9e758e5d998d80fbf3db2636751")
            .unwrap()
    );
}
