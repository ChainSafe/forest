// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![cfg(feature = "lumberjackdb")]

mod subtests;

use forest_db::lumberjack::LumberjackDb;

#[test]
fn lumberjack_db_write() {
    let db = LumberjackDb::temporary().unwrap();
    subtests::write(&db);
}

#[test]
fn lumberjack_db_read() {
    let db = LumberjackDb::temporary().unwrap();
    subtests::read(&db);
}

#[test]
fn lumberjack_db_exists() {
    let db = LumberjackDb::temporary().unwrap();
    subtests::exists(&db);
}

#[test]
fn lumberjack_db_does_not_exist() {
    let db = LumberjackDb::temporary().unwrap();
    subtests::does_not_exist(&db);
}

#[test]
fn lumberjack_db_delete() {
    let db = LumberjackDb::temporary().unwrap();
    subtests::delete(&db);
}

#[test]
fn lumberjack_db_bulk_write() {
    let db = LumberjackDb::temporary().unwrap();
    subtests::bulk_write(&db);
}

#[test]
fn lumberjack_db_bulk_read() {
    let db = LumberjackDb::temporary().unwrap();
    subtests::bulk_read(&db);
}

#[test]
fn lumberjack_db_bulk_delete() {
    let db = LumberjackDb::temporary().unwrap();
    subtests::bulk_delete(&db);
}
