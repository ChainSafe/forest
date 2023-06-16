// Copyright 2019-2023 ChainSafe Systems
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
    assert!(res);
}

pub fn does_not_exist<DB>(db: &DB)
where
    DB: Store,
{
    let key = [0];
    let res = db.exists(key).unwrap();
    assert!(!res);
}

pub fn bulk_write<DB>(db: &DB)
where
    DB: Store,
{
    let values = [([0], [0]), ([1], [1]), ([2], [2])];
    db.bulk_write(values).unwrap();
    for (k, _) in values.iter() {
        let res = db.exists(*k).unwrap();
        assert!(res);
    }
}
