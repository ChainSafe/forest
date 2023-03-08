// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod errors;
mod memory;
mod metrics;

#[cfg(feature = "rocksdb")]
pub mod rocks;

#[cfg(feature = "paritydb")]
pub mod parity_db;

pub mod parity_db_config;
pub mod rocks_config;

pub use errors::Error;
pub use memory::MemoryDB;

#[cfg(any(feature = "paritydb", feature = "rocksdb"))]
pub mod rolling;

/// Store interface used as a KV store implementation
pub trait Store {
    /// Read single value from data store and return `None` if key doesn't
    /// exist.
    fn read<K>(&self, key: K) -> Result<Option<Vec<u8>>, Error>
    where
        K: AsRef<[u8]>;

    /// Write a single value to the data store.
    fn write<K, V>(&self, key: K, value: V) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>;

    /// Delete value at key.
    fn delete<K>(&self, key: K) -> Result<(), Error>
    where
        K: AsRef<[u8]>;

    /// Returns `Ok(true)` if key exists in store
    fn exists<K>(&self, key: K) -> Result<bool, Error>
    where
        K: AsRef<[u8]>;

    /// Write slice of KV pairs.
    fn bulk_write(
        &self,
        values: impl IntoIterator<Item = (impl Into<Vec<u8>>, impl Into<Vec<u8>>)>,
    ) -> Result<(), Error> {
        values
            .into_iter()
            .try_for_each(|(key, value)| self.write(key.into(), value.into()))
    }

    /// Flush writing buffer if there is any. Default implementation is blank
    fn flush(&self) -> Result<(), Error> {
        Ok(())
    }
}

impl<BS: Store> Store for &BS {
    fn read<K>(&self, key: K) -> Result<Option<Vec<u8>>, Error>
    where
        K: AsRef<[u8]>,
    {
        (*self).read(key)
    }

    fn write<K, V>(&self, key: K, value: V) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        (*self).write(key, value)
    }

    fn delete<K>(&self, key: K) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
    {
        (*self).delete(key)
    }

    fn exists<K>(&self, key: K) -> Result<bool, Error>
    where
        K: AsRef<[u8]>,
    {
        (*self).exists(key)
    }

    fn bulk_write(
        &self,
        values: impl IntoIterator<Item = (impl Into<Vec<u8>>, impl Into<Vec<u8>>)>,
    ) -> Result<(), Error> {
        (*self).bulk_write(values)
    }
}

/// Traits for collecting DB stats
pub trait DBStatistics {
    fn get_statistics(&self) -> Option<String> {
        None
    }
}

#[cfg(feature = "rocksdb")]
pub mod db_engine {
    use std::path::{Path, PathBuf};

    pub type Db = crate::rocks::RocksDb;
    pub type DbConfig = crate::rocks_config::RocksDbConfig;

    pub fn db_path(path: &Path) -> PathBuf {
        path.join("rocksdb")
    }

    pub fn open_db(path: &std::path::Path, config: &DbConfig) -> anyhow::Result<Db> {
        crate::rocks::RocksDb::open(path, config).map_err(Into::into)
    }
}

#[cfg(feature = "paritydb")]
pub mod db_engine {
    use std::path::{Path, PathBuf};

    pub type Db = crate::parity_db::ParityDb;
    pub type DbConfig = crate::parity_db_config::ParityDbConfig;

    pub fn db_path(path: &Path) -> PathBuf {
        path.join("paritydb")
    }

    pub fn open_db(path: &std::path::Path, config: &DbConfig) -> anyhow::Result<Db> {
        use crate::parity_db::ParityDb;
        ParityDb::open(path.to_owned(), config)
    }
}
