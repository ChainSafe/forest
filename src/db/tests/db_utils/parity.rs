// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::ops::Deref;

use crate::db::{parity_db::ParityDb, parity_db_config::ParityDbConfig};

/// Temporary, self-cleaning ParityDB
pub struct TempParityDB {
    db: Option<ParityDb>,
    _dir: tempfile::TempDir, // kept for cleaning up during Drop
}

impl TempParityDB {
    /// Creates a new DB in a temporary path that gets wiped out when the
    /// variable gets out of scope.
    pub fn new() -> TempParityDB {
        let dir = tempfile::Builder::new()
            .tempdir()
            .expect("Failed to create temporary path for db.");
        let path = dir.path().join("paritydb");
        let config = ParityDbConfig::default();

        TempParityDB {
            db: Some(ParityDb::open(path, &config).unwrap()),
            _dir: dir,
        }
    }
}

impl Deref for TempParityDB {
    type Target = ParityDb;

    fn deref(&self) -> &Self::Target {
        self.db.as_ref().unwrap()
    }
}

impl AsRef<ParityDb> for TempParityDB {
    fn as_ref(&self) -> &ParityDb {
        self.db.as_ref().unwrap()
    }
}
