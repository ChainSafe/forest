// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::errors::Error;
use super::Store;
pub use sled::{Batch, Config, Db, Mode};
use std::path::Path;

/// Sled instance this satisfies the [Store] interface.
#[derive(Debug)]
pub struct SledDb {
    pub db: Db,
}

/// SledDb is an alternative blockstore implementation to rocksdb.
/// This is experimental for now and should not be used as a default.
///
/// Usage:
/// ```no_run
/// use forest_db::sled::SledDb;
///
/// let mut db = SledDb::open("test_db");
/// ```
impl SledDb {
    pub fn open<P>(path: P) -> Result<Self, Error>
    where
        P: AsRef<Path>,
    {
        let options = Config::default()
            .path(path)
            .mode(sled::Mode::HighThroughput)
            // 4 gb
            .cache_capacity(1024 * 1024 * 1024 * 4);
        Ok(Self {
            db: options.open()?,
        })
    }

    /// Open a db with custom configuration.
    pub fn open_with_config(config: Config) -> Result<Self, Error> {
        Ok(Self { db: config.open()? })
    }

    /// Initialize a sled in memory database. This will not persist data.
    pub fn temporary() -> Result<Self, Error> {
        let options = sled::Config::default().temporary(true);
        Ok(Self {
            db: options.open()?,
        })
    }
}

impl Store for SledDb {
    fn write<K, V>(&self, key: K, value: V) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        self.db.insert(key, value.as_ref())?;
        Ok(())
    }

    fn delete<K>(&self, key: K) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
    {
        self.db.remove(key)?;
        Ok(())
    }

    fn read<K>(&self, key: K) -> Result<Option<Vec<u8>>, Error>
    where
        K: AsRef<[u8]>,
    {
        Ok(self.db.get(key)?.map(|v| v.as_ref().into()))
    }

    fn exists<K>(&self, key: K) -> Result<bool, Error>
    where
        K: AsRef<[u8]>,
    {
        Ok(self.db.contains_key(key)?)
    }
}
