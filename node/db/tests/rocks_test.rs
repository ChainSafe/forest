// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[cfg(feature = "rocksdb")]
mod db_utils;

#[cfg(feature = "rocksdb")]
mod subtests;

#[cfg(feature = "rocksdb")]
mod rocksdb_tests {
    use super::*;
    use crate::db_utils::rocks::TempRocksDB;

    #[test]
    fn db_write() {
        let db = TempRocksDB::new();
        subtests::write(&*db);
    }

    #[test]
    fn db_read() {
        let db = TempRocksDB::new();
        subtests::read(&*db);
    }

    #[test]
    fn db_exists() {
        let db = TempRocksDB::new();
        subtests::exists(&*db);
    }

    #[test]
    fn db_does_not_exist() {
        let db = TempRocksDB::new();
        subtests::does_not_exist(&*db);
    }

    #[test]
    fn db_delete() {
        let db = TempRocksDB::new();
        subtests::delete(&*db);
    }

    #[test]
    fn db_bulk_write() {
        let db = TempRocksDB::new();
        subtests::bulk_write(&*db);
    }

    #[test]
    fn db_bulk_read() {
        let db = TempRocksDB::new();
        subtests::bulk_read(&*db);
    }

    #[test]
    fn db_bulk_delete() {
        let db = TempRocksDB::new();
        subtests::bulk_delete(&*db);
    }
}
