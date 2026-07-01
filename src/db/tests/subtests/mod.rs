// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::db::{EthBlockBloomStore, SettingsStore, SettingsStoreExt};
use crate::utils::multihash::prelude::*;
use cid::Cid;
use fvm_ipld_encoding::DAG_CBOR;

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

pub fn block_bloom_prune<DB>(db: &DB)
where
    DB: EthBlockBloomStore,
{
    let a = Cid::new_v1(DAG_CBOR, MultihashCode::Blake2b256.digest(b"a"));
    let b = Cid::new_v1(DAG_CBOR, MultihashCode::Blake2b256.digest(b"b"));
    let missing = Cid::new_v1(DAG_CBOR, MultihashCode::Blake2b256.digest(b"missing"));
    let bloom_a = vec![0x11; 256];
    let bloom_b = vec![0x22; 256];

    db.write_bloom(&a, 100, &bloom_a).unwrap();
    db.write_bloom(&b, 200, &bloom_b).unwrap();
    assert_eq!(db.read_bloom(&a).unwrap().as_deref(), Some(bloom_a.as_slice()));
    assert_eq!(db.read_bloom(&b).unwrap().as_deref(), Some(bloom_b.as_slice()));
    assert_eq!(db.read_bloom(&missing).unwrap(), None);

    // Only entries at or above the cutoff survive.
    db.delete_blooms_before_height(150).unwrap();
    assert_eq!(db.read_bloom(&a).unwrap(), None);
    assert_eq!(db.read_bloom(&b).unwrap().as_deref(), Some(bloom_b.as_slice()));
}
