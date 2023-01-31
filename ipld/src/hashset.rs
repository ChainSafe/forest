// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use ahash::{HashSet, HashSetExt};
use forest_db::{db_engine, ReadStore, ReadWriteStore};
use std::mem::size_of;
use tempfile::TempDir;

// type Hash = blake3::Hash;
type Hash = md5::Digest;

fn hash(data: impl AsRef<[u8]>) -> Hash {
    // blake3::hash(data.as_ref())
    md5::compute(data)
}

pub struct DbBackedHashSet {
    mem: HashSet<Hash>,
    db: db_engine::Db,
    // Drop dir after db
    dir: TempDir,
    // hash_key: bool,
}

impl DbBackedHashSet {
    pub fn new() -> anyhow::Result<Self> {
        let mem = HashSet::new();
        let dir = tempfile::tempdir()?;
        log::info!("Creating temp DB at {}", dir.path().display());
        let db = db_engine::Db::open(dir.path().into(), &Default::default())?;
        Ok(DbBackedHashSet { mem, db, dir })
    }

    pub fn insert(&mut self, key: impl AsRef<[u8]>) -> anyhow::Result<bool> {
        // 4GB
        const CAPACITY: usize = 4 * 1024 * 1024 * 1024 / size_of::<Hash>();

        let hash = hash(key.as_ref());

        if self.mem.len() < CAPACITY {
            Ok(self.mem.insert(hash))
        } else if self.mem.contains(&hash) {
            Ok(false)
        } else {
            let key = &hash.0; // .as_bytes();
            if self.db.exists(key)? {
                Ok(false)
            } else {
                self.db.write(key, [])?;
                Ok(true)
            }
        }
    }
}

impl Drop for DbBackedHashSet {
    fn drop(&mut self) {
        use human_repr::HumanCount;

        let size = fs_extra::dir::get_size(self.dir.path()).unwrap_or_default();
        log::info!(
            "Cleaning up temp DB at {}, size: {}",
            self.dir.path().display(),
            size.human_count_bytes()
        );
    }
}
