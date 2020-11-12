// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![cfg(feature = "rocksdb")]

mod db_utils;
mod subtests;

use db::RocksDb;
use db_utils::DBPath;

#[test]
fn rocks_db_write() {
    let path = DBPath::new("write_rocks_test");
    let mut db = RocksDb::new(path.as_ref());
    db.open().unwrap();
    subtests::write(&db);
}

#[test]
fn rocks_db_read() {
    let path = DBPath::new("read_rocks_test");
    let mut db = RocksDb::new(path.as_ref());
    db.open().unwrap();
    subtests::read(&db);
}

#[test]
fn rocks_db_exists() {
    let path = DBPath::new("exists_rocks_test");
    let mut db = RocksDb::new(path.as_ref());
    db.open().unwrap();
    subtests::exists(&db);
}

#[test]
fn rocks_db_does_not_exist() {
    let path = DBPath::new("does_not_exists_rocks_test");
    let mut db = RocksDb::new(path.as_ref());
    db.open().unwrap();
    subtests::does_not_exist(&db);
}

#[test]
fn rocks_db_delete() {
    let path = DBPath::new("delete_rocks_test");
    let mut db = RocksDb::new(path.as_ref());
    db.open().unwrap();
    subtests::delete(&db);
}

#[test]
fn rocks_db_bulk_write() {
    let path = DBPath::new("bulk_write_rocks_test");
    let mut db = RocksDb::new(path.as_ref());
    db.open().unwrap();
    subtests::bulk_write(&db);
}

#[test]
fn rocks_db_bulk_read() {
    let path = DBPath::new("bulk_read_rocks_test");
    let mut db = RocksDb::new(path.as_ref());
    db.open().unwrap();
    subtests::bulk_read(&db);
}

#[test]
fn rocks_db_bulk_delete() {
    let path = DBPath::new("bulk_delete_rocks_test");
    let mut db = RocksDb::new(path.as_ref());
    db.open().unwrap();
    subtests::bulk_delete(&db);
}
