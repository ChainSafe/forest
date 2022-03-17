// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::errors::Error;
use super::Store;
use num_cpus;
pub use rocksdb::{Options, WriteBatch, DB};
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct RocksDbConfig {
    create_if_missing: bool,
    parallelism: i32,
    write_buffer_size: usize,
    max_open_files: i32,
}

impl Default for RocksDbConfig {
    fn default() -> Self {
        Self {
            create_if_missing: true,
            parallelism: num_cpus::get() as i32,
            write_buffer_size: 256 * 1024 * 1024,
            max_open_files: 200,
        }
    }
}

/// RocksDB instance this satisfies the [Store] interface.
#[derive(Debug)]
pub struct RocksDb {
    pub db: DB,
}

/// RocksDb is used as the KV store for Forest
///
/// Usage:
/// ```no_run
/// use forest_db::rocks::{RocksDb, RocksDbConfig};
///
/// let mut db = RocksDb::open("test_db", RocksDbConfig::default()).unwrap();
/// ```
impl RocksDb {
    pub fn open<P>(path: P, config: RocksDbConfig) -> Result<Self, Error>
    where
        P: AsRef<Path>,
    {
        let mut db_opts = Options::default();
        db_opts.create_if_missing(config.create_if_missing);
        db_opts.increase_parallelism(config.parallelism);
        db_opts.set_write_buffer_size(config.write_buffer_size);
        db_opts.set_max_open_files(config.max_open_files);
        Ok(Self {
            db: DB::open(&db_opts, path)?,
        })
    }
}

impl Store for RocksDb {
    fn read<K>(&self, key: K) -> Result<Option<Vec<u8>>, Error>
    where
        K: AsRef<[u8]>,
    {
        self.db.get(key).map_err(Error::from)
    }

    fn write<K, V>(&self, key: K, value: V) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        Ok(self.db.put(key, value)?)
    }

    fn delete<K>(&self, key: K) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
    {
        Ok(self.db.delete(key)?)
    }

    fn exists<K>(&self, key: K) -> Result<bool, Error>
    where
        K: AsRef<[u8]>,
    {
        self.db
            .get_pinned(key)
            .map(|v| v.is_some())
            .map_err(Error::from)
    }

    fn bulk_write<K, V>(&self, values: &[(K, V)]) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        let mut batch = WriteBatch::default();
        for (k, v) in values {
            batch.put(k, v);
        }
        Ok(self.db.write(batch)?)
    }
}
