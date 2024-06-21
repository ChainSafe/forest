// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::ops::Deref;
use std::sync::Arc;

use crate::db::{parity_db::ParityDb, parity_db_config::ParityDbConfig};

/// Temporary, self-cleaning ParityDB
pub struct TempParityDB {
    db: Arc<ParityDb>,
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
            db: Arc::new(ParityDb::open(path, &config).unwrap()),
            _dir: dir,
        }
    }

    /// Note that all the `Arc<ParityDB>` should be dropped before `TempParityDB` cleanup.
    pub fn arc(&self) -> Arc<ParityDb> {
        self.db.clone()
    }
}

impl Deref for TempParityDB {
    type Target = ParityDb;

    fn deref(&self) -> &Self::Target {
        self.db.as_ref()
    }
}

impl AsRef<ParityDb> for TempParityDB {
    fn as_ref(&self) -> &ParityDb {
        self.db.as_ref()
    }
}
