// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::db::{SettingsStore, SettingsStoreExt};

pub fn write_bin<DB>(db: &DB)
where
    DB: SettingsStore,
{
    let key = "1";
    let value = [1];
    db.write_bin(key, &value).unwrap();
}

pub fn read_bin<DB>(db: &DB)
where
    DB: SettingsStore,
{
    let key = "0";
    let value = [1];
    db.write_bin(key, &value).unwrap();
    let res = db.read_bin(key).unwrap().unwrap();
    assert_eq!(value.as_ref(), res.as_slice());
}

pub fn write_read_obj<DB>(db: &DB)
where
    DB: SettingsStore,
{
    let key = "Cthulhu";
    let value = 42;
    db.write_obj(key, &value).unwrap();
    let res: i32 = db.read_obj(key).unwrap().unwrap();
    assert_eq!(value, res);

    // ensure that we are able to overwrite the value.
    // this is to ensure we don't enable settings such as
    // `preimage` for the settings column which would
    // assume that the value is immutable.
    let value = 1337;
    db.write_obj(key, &value).unwrap();
    let res: i32 = db.read_obj(key).unwrap().unwrap();
    assert_eq!(value, res);
}

pub fn exists<DB>(db: &DB)
where
    DB: SettingsStore,
{
    let key = "0";
    let value = [1];
    db.write_bin(key, &value).unwrap();
    let res = db.exists(key).unwrap();
    assert!(res);
}

pub fn does_not_exist<DB>(db: &DB)
where
    DB: SettingsStore,
{
    let key = "Azathoth";

    assert!(!db.exists(key).unwrap());
    assert!(db.read_obj::<i32>(key).unwrap().is_none());
    assert!(db.require_obj::<i32>(key).is_err());
}
