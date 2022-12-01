// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::ops::Deref;

use forest_db::lmdb::{LMDb, LMDbConfig};

/// Temporary, self-cleaning LMDb
pub struct TempLMDb {
    db: LMDb,
    _dir: tempfile::TempDir, // kept for cleaning up during Drop
}

impl TempLMDb {
    /// Creates a new DB in a temporary path that gets wiped out when the variable
    /// gets out of scope.
    pub fn new() -> TempLMDb {
        let dir = tempfile::Builder::new()
            .tempdir()
            .expect("Failed to create temporary path for db.");
        let path = dir.path();
        let config = LMDbConfig::from_path(&path);
        TempLMDb {
            db: LMDb::open(&config).unwrap(),
            _dir: dir,
        }
    }
}

impl Deref for TempLMDb {
    type Target = LMDb;

    fn deref(&self) -> &Self::Target {
        &self.db
    }
}
