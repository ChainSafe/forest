// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![cfg(feature = "rocksdb")]

use super::errors::Error;
use super::{DatabaseService, Store};
use rocksdb::{Options, WriteBatch, DB};
use std::env::temp_dir;
use std::path::{Path, PathBuf};

#[derive(Debug)]
enum DbStatus {
    Unopened(PathBuf),
    Open(DB),
}

impl Default for DbStatus {
    fn default() -> Self {
        Self::Unopened(Path::new(&temp_dir()).to_path_buf())
    }
}

#[derive(Debug, Default)]
pub struct RocksDb {
    status: DbStatus,
}

/// RocksDb is used as the KV store for Forest
///
/// Usage:
/// ```no_run
/// use db::RocksDb;
///
/// let mut db = RocksDb::new("test_db");
/// db.open();
/// ```
impl RocksDb {
    pub fn new<P>(path: P) -> Self
    where
        P: AsRef<Path>,
    {
        Self {
            status: DbStatus::Unopened(path.as_ref().to_path_buf()),
        }
    }

    /// Initializes the database if uninitialized, does nothing if db is already opened
    pub fn open(&mut self) -> Result<(), Error> {
        match &self.status {
            DbStatus::Unopened(path) => {
                let mut db_opts = Options::default();
                db_opts.create_if_missing(true);
                self.status = DbStatus::Open(DB::open(&db_opts, path)?);
                Ok(())
            }
            DbStatus::Open(_) => Ok(()),
        }
    }

    /// Returns reference to db as long as it is initialized
    pub fn db(&self) -> Result<&DB, Error> {
        match &self.status {
            DbStatus::Unopened(_) => Err(Error::Unopened),
            DbStatus::Open(db) => Ok(db),
        }
    }
}

impl DatabaseService for RocksDb {
    fn open(&mut self) -> Result<(), Error> {
        self.open()
    }
}

impl Store for RocksDb {
    fn write<K, V>(&self, key: K, value: V) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        Ok(self.db()?.put(key, value)?)
    }

    fn delete<K>(&self, key: K) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
    {
        Ok(self.db()?.delete(key)?)
    }

    fn bulk_write<K, V>(&self, keys: &[K], values: &[V]) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        // Safety check to make sure kv lengths are the same
        if keys.len() != values.len() {
            return Err(Error::InvalidBulkLen);
        }

        let mut batch = WriteBatch::default();
        for (k, v) in keys.iter().zip(values.iter()) {
            batch.put(k, v);
        }
        Ok(self.db()?.write(batch)?)
    }

    fn bulk_delete<K>(&self, keys: &[K]) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
    {
        for k in keys.iter() {
            self.db()?.delete(k)?;
        }
        Ok(())
    }

    fn read<K>(&self, key: K) -> Result<Option<Vec<u8>>, Error>
    where
        K: AsRef<[u8]>,
    {
        self.db()?.get(key).map_err(Error::from)
    }

    fn exists<K>(&self, key: K) -> Result<bool, Error>
    where
        K: AsRef<[u8]>,
    {
        self.db()?
            .get_pinned(key)
            .map(|v| v.is_some())
            .map_err(Error::from)
    }

    fn bulk_read<K>(&self, keys: &[K]) -> Result<Vec<Option<Vec<u8>>>, Error>
    where
        K: AsRef<[u8]>,
    {
        let mut v = Vec::with_capacity(keys.len());
        for k in keys.iter() {
            match self.db()?.get(k) {
                Ok(val) => v.push(val),
                Err(e) => return Err(Error::from(e)),
            }
        }
        Ok(v)
    }
}
