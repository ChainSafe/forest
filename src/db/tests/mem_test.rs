// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::subtests;

use crate::db::MemoryDB;

#[test]
fn mem_db_write() {
    let db = MemoryDB::default();
    subtests::write_bin(&db);
}

#[test]
fn mem_db_read() {
    let db = MemoryDB::default();
    subtests::read_bin(&db);
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
fn mem_write_read_obj() {
    let db = MemoryDB::default();
    subtests::write_read_obj(&db);
}
