// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{db_utils::parity::TempParityDB, subtests};

#[test]
fn db_write() {
    let db = TempParityDB::new();
    subtests::write_bin(&*db);
}

#[test]
fn db_read() {
    let db = TempParityDB::new();
    subtests::read_bin(&*db);
}

#[test]
fn db_exists() {
    let db = TempParityDB::new();
    subtests::exists(&*db);
}

#[test]
fn db_does_not_exist() {
    let db = TempParityDB::new();
    subtests::does_not_exist(&*db);
}

#[test]
fn db_write_read_obj() {
    let db = TempParityDB::new();
    subtests::write_read_obj(&*db);
}
