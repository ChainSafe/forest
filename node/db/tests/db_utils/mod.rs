// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![cfg(feature = "rocksdb")]

use forest_db::rocks::RocksDb;
use forest_db::rocks_config::RocksDbConfig;
use std::ops::Deref;

/// Temporary, self-cleaning RocksDB
pub struct TempRocksDB {
    db: RocksDb,
    _dir: tempfile::TempDir, // kept for cleaning up during Drop
}

impl TempRocksDB {
    /// Creates a new DB in a temporary path that gets wiped out when the variable
    /// gets out of scope.
    pub fn new() -> TempRocksDB {
        let dir = tempfile::Builder::new()
            .tempdir()
            .expect("Failed to create temporary path for db.");
        let path = dir.path().join("db");

        TempRocksDB {
            db: RocksDb::open(&path, &RocksDbConfig::default()).unwrap(),
            _dir: dir,
        }
    }
}

impl Deref for TempRocksDB {
    type Target = RocksDb;

    fn deref(&self) -> &Self::Target {
        &self.db
    }
}
