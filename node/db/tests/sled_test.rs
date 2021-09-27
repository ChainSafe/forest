// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![cfg(feature = "sled")]

mod subtests;

use forest_db::sled::SledDb;

#[test]
fn sled_db_write() {
    let db = SledDb::temporary().unwrap();
    subtests::write(&db);
}

#[test]
fn sled_db_read() {
    let db = SledDb::temporary().unwrap();
    subtests::read(&db);
}

#[test]
fn sled_db_exists() {
    let db = SledDb::temporary().unwrap();
    subtests::exists(&db);
}

#[test]
fn sled_db_does_not_exist() {
    let db = SledDb::temporary().unwrap();
    subtests::does_not_exist(&db);
}

#[test]
fn sled_db_delete() {
    let db = SledDb::temporary().unwrap();
    subtests::delete(&db);
}

#[test]
fn sled_db_bulk_write() {
    let db = SledDb::temporary().unwrap();
    subtests::bulk_write(&db);
}

#[test]
fn sled_db_bulk_read() {
    let db = SledDb::temporary().unwrap();
    subtests::bulk_read(&db);
}

#[test]
fn sled_db_bulk_delete() {
    let db = SledDb::temporary().unwrap();
    subtests::bulk_delete(&db);
}
