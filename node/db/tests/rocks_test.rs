// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![cfg(feature = "rocksdb")]

mod db_utils;
mod subtests;

use crate::db_utils::TempRocksDB;

#[test]
fn rocks_db_write() {
    let db = TempRocksDB::new();
    subtests::write(&*db);
}

#[test]
fn rocks_db_read() {
    let db = TempRocksDB::new();
    subtests::read(&*db);
}

#[test]
fn rocks_db_exists() {
    let db = TempRocksDB::new();
    subtests::exists(&*db);
}

#[test]
fn rocks_db_does_not_exist() {
    let db = TempRocksDB::new();
    subtests::does_not_exist(&*db);
}

#[test]
fn rocks_db_delete() {
    let db = TempRocksDB::new();
    subtests::delete(&*db);
}

#[test]
fn rocks_db_bulk_write() {
    let db = TempRocksDB::new();
    subtests::bulk_write(&*db);
}

#[test]
fn rocks_db_bulk_read() {
    let db = TempRocksDB::new();
    subtests::bulk_read(&*db);
}

#[test]
fn rocks_db_bulk_delete() {
    let db = TempRocksDB::new();
    subtests::bulk_delete(&*db);
}
