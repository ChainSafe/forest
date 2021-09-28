// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod subtests;

use forest_db::MemoryDB;

#[test]
fn mem_db_write() {
    let db = MemoryDB::default();
    subtests::write(&db);
}

#[test]
fn mem_db_read() {
    let db = MemoryDB::default();
    subtests::read(&db);
}

#[test]
fn mem_db_exists() {
    let db = MemoryDB::default();
    subtests::exists(&db);
}

#[test]
fn mem_db_does_not_exist() {
    let db = MemoryDB::default();
    subtests::does_not_exist(&db);
}

#[test]
fn mem_db_delete() {
    let db = MemoryDB::default();
    subtests::delete(&db);
}

#[test]
fn mem_db_bulk_write() {
    let db = MemoryDB::default();
    subtests::bulk_write(&db);
}

#[test]
fn mem_db_bulk_read() {
    let db = MemoryDB::default();
    subtests::bulk_read(&db);
}

#[test]
fn mem_db_bulk_delete() {
    let db = MemoryDB::default();
    subtests::bulk_delete(&db);
}
