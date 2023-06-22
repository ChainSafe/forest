// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod errors;
mod memory;
mod metrics;
pub mod parity_db;
pub mod parity_db_config;
pub use errors::Error;
pub use memory::MemoryDB;

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

pub mod db_engine {
    use std::path::{Path, PathBuf};

    use crate::db::rolling::*;

    pub type Db = crate::db::parity_db::ParityDb;
    pub type DbConfig = crate::db::parity_db_config::ParityDbConfig;
    pub(in crate::db) type DbError = parity_db::Error;
    const DIR_NAME: &str = "paritydb";

    pub fn db_root(chain_data_root: &Path) -> PathBuf {
        chain_data_root.join(DIR_NAME)
    }

    pub(in crate::db) fn open_db(path: &Path, config: &DbConfig) -> anyhow::Result<Db> {
        Db::open(path, config).map_err(Into::into)
    }

    pub fn open_proxy_db(db_root: PathBuf, db_config: DbConfig) -> anyhow::Result<RollingDB> {
        RollingDB::load_or_create(db_root, db_config)
    }
}
#[cfg(test)]
mod tests {
    pub mod db_utils;
    mod mem_test;
    mod parity_test;
    pub mod subtests;
}
