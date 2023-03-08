// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod impls;
mod index;

use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    time::SystemTime,
};

use chrono::Utc;
use fvm_ipld_blockstore::Blockstore;

use crate::db_engine::{open_db, Db, DbConfig};

pub struct RollingDB {
    db_root: PathBuf,
    db_config: DbConfig,
    dbs: VecDeque<Db>,
}

impl RollingDB {
    pub fn load_or_create(db_root: PathBuf, db_config: DbConfig) -> anyhow::Result<Self> {
        let dbs = load_dbs(db_root.as_path());
        let mut rolling = Self {
            db_root,
            db_config,
            dbs,
        };

        if rolling.dbs.is_empty() {
            let name = Utc::now().timestamp();
            let db = open_db(&rolling.db_root.join(name.to_string()), &rolling.db_config)?;
            rolling.add_as_current(db)?;
        }

        Ok(rolling)
    }

    pub fn add_as_current(&mut self, db: Db) -> anyhow::Result<()> {
        self.dbs.push_front(db);
        self.flush_index_to_file()
    }

    fn current(&self) -> &Db {
        self.dbs
            .get(0)
            .expect("RollingDB should contain at least one DB reference")
    }

    fn flush_index_to_file(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

fn load_dbs(db_root: &Path) -> VecDeque<Db> {
    todo!()
}

fn get_db_index_name(db_root: &Path) -> PathBuf {
    db_root.join("db_index.yaml")
}
