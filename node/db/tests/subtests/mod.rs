// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_db::Store;

pub fn write<DB>(db: &DB)
where
    DB: Store,
{
    let key = [1];
    let value = [1];
    db.write(key, value).unwrap();
}

pub fn read<DB>(db: &DB)
where
    DB: Store,
{
    let key = [0];
    let value = [1];
    db.write(key, value).unwrap();
    let res = db.read(key).unwrap().unwrap();
    assert_eq!(value.as_ref(), res.as_slice());
}

pub fn exists<DB>(db: &DB)
where
    DB: Store,
{
    let key = [0];
    let value = [1];
    db.write(key, value).unwrap();
    let res = db.exists(key).unwrap();
    assert_eq!(res, true);
}

pub fn does_not_exist<DB>(db: &DB)
where
    DB: Store,
{
    let key = [0];
    let res = db.exists(key).unwrap();
    assert_eq!(res, false);
}

pub fn delete<DB>(db: &DB)
where
    DB: Store,
{
    let key = [0];
    let value = [1];
    db.write(key, value).unwrap();
    let res = db.exists(key).unwrap();
    assert_eq!(res, true);
    db.delete(key).unwrap();
    let res = db.exists(key).unwrap();
    assert_eq!(res, false);
}

pub fn bulk_write<DB>(db: &DB)
where
    DB: Store,
{
    let values = [([0], [0]), ([1], [1]), ([2], [2])];
    db.bulk_write(&values).unwrap();
    for (k, _) in values.iter() {
        let res = db.exists(*k).unwrap();
        assert_eq!(res, true);
    }
}

pub fn bulk_read<DB>(db: &DB)
where
    DB: Store,
{
    let keys = [[0], [1], [2]];
    let values = [[0], [1], [2]];
    let kvs: Vec<_> = keys.iter().zip(values.iter()).collect();
    db.bulk_write(&kvs).unwrap();
    let results = db.bulk_read(&keys).unwrap();
    for (result, value) in results.iter().zip(values.iter()) {
        match result {
            Some(v) => assert_eq!(v, value),
            None => panic!("No values found!"),
        }
    }
}

pub fn bulk_delete<DB>(db: &DB)
where
    DB: Store,
{
    let keys = [[0], [1], [2]];
    let values = [[0], [1], [2]];
    let kvs: Vec<_> = keys.iter().zip(values.iter()).collect();
    db.bulk_write(&kvs).unwrap();
    db.bulk_delete(&keys).unwrap();
    for k in keys.iter() {
        let res = db.exists(*k).unwrap();
        assert_eq!(res, false);
    }
}
