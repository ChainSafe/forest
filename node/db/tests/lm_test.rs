// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[cfg(feature = "lmdb")]
mod db_utils;

#[cfg(feature = "lmdb")]
mod subtests;

#[cfg(feature = "lmdb")]
mod lmdb_tests {
    use super::*;
    use crate::db_utils::lmdb::TempLMDb;

    #[test]
    fn db_write() {
        let db = TempLMDb::new();
        subtests::write(&*db);
    }

    #[test]
    fn db_read() {
        let db = TempLMDb::new();
        subtests::read(&*db);
    }

    #[test]
    fn db_exists() {
        let db = TempLMDb::new();
        subtests::exists(&*db);
    }

    #[test]
    fn db_does_not_exist() {
        let db = TempLMDb::new();
        subtests::does_not_exist(&*db);
    }

    #[test]
    fn db_delete() {
        let db = TempLMDb::new();
        subtests::delete(&*db);
    }

    #[test]
    fn db_bulk_write() {
        let db = TempLMDb::new();
        subtests::bulk_write(&*db);
    }

    #[test]
    fn db_bulk_read() {
        let db = TempLMDb::new();
        subtests::bulk_read(&*db);
    }

    #[test]
    fn db_bulk_delete() {
        let db = TempLMDb::new();
        subtests::bulk_delete(&*db);
    }
}
