// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::errors::Error;
use super::Store;
pub use rocksdb::{Options, WriteBatch, DB};
use std::path::Path;

/// RocksDB instance this satisfies the [Store] interface.
#[derive(Debug)]
pub struct RocksDb {
    pub db: DB,
}

/// RocksDb is used as the KV store for Forest
///
/// Usage:
/// ```no_run
/// use forest_db::rocks::RocksDb;
///
/// let mut db = RocksDb::open("test_db").unwrap();
/// ```
impl RocksDb {
    pub fn open<P>(path: P) -> Result<Self, Error>
    where
        P: AsRef<Path>,
    {
        let mut db_opts = Options::default();
        db_opts.create_if_missing(true);
        Ok(Self {
            db: DB::open(&db_opts, path)?,
        })
    }
}

impl Store for RocksDb {
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

    fn read<K>(&self, key: K) -> Result<Option<Vec<u8>>, Error>
    where
        K: AsRef<[u8]>,
    {
        self.db.get(key).map_err(Error::from)
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
}
