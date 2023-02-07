// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod errors;
mod memory;
mod metrics;

pub mod rolling;

#[cfg(feature = "rocksdb")]
pub mod rocks;

#[cfg(feature = "paritydb")]
pub mod parity_db;

pub mod parity_db_config;
pub mod rocks_config;

use std::sync::Arc;

pub use errors::Error;
pub use memory::MemoryDB;
use rolling::{ProxyStore, RollingStore, SplitStore, TrackingStore};

/// Read-only store interface used as a KV store implementation
pub trait ReadStore {
    /// Read single value from data store and return `None` if key doesn't
    /// exist.
    fn read<K>(&self, key: K) -> Result<Option<Vec<u8>>, Error>
    where
        K: AsRef<[u8]>;

    /// Returns `Ok(true)` if key exists in store
    fn exists<K>(&self, key: K) -> Result<bool, Error>
    where
        K: AsRef<[u8]>;

    /// Read slice of keys and return a vector of optional values.
    fn bulk_read<K>(&self, keys: &[K]) -> Result<Vec<Option<Vec<u8>>>, Error>
    where
        K: AsRef<[u8]>,
    {
        keys.iter().map(|key| self.read(key)).collect()
    }
}

/// Store interface used as a KV store implementation
pub trait ReadWriteStore: ReadStore {
    /// Write a single value to the data store.
    fn write<K, V>(&self, key: K, value: V) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>;

    /// Delete value at key.
    fn delete<K>(&self, key: K) -> Result<(), Error>
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

    /// Bulk delete keys from the data store.
    fn bulk_delete<K>(&self, keys: &[K]) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
    {
        keys.iter().try_for_each(|key| self.delete(key))
    }

    /// Flush writing buffer if there is any. Default implementation is blank
    fn flush(&self) -> Result<(), Error> {
        Ok(())
    }
}

pub trait Store: ReadWriteStore {
    fn persistent(&self) -> &db_engine::Db;

    fn rolling(&self) -> &RollingStore<crate::db_engine::Db>;

    fn rolling_by_epoch(
        &self,
        epoch: i64,
    ) -> SplitStore<ProxyStore<crate::db_engine::Db>, TrackingStore<crate::db_engine::Db>>;

    fn rolling_by_epoch_raw(&self, epoch: i64) -> TrackingStore<crate::db_engine::Db>;

    fn rolling_stats(&self) -> String;
}

impl<BS: ReadStore> ReadStore for &BS {
    fn read<K>(&self, key: K) -> Result<Option<Vec<u8>>, Error>
    where
        K: AsRef<[u8]>,
    {
        (*self).read(key)
    }

    fn exists<K>(&self, key: K) -> Result<bool, Error>
    where
        K: AsRef<[u8]>,
    {
        (*self).exists(key)
    }

    fn bulk_read<K>(&self, keys: &[K]) -> Result<Vec<Option<Vec<u8>>>, Error>
    where
        K: AsRef<[u8]>,
    {
        (*self).bulk_read(keys)
    }
}

impl<BS: ReadWriteStore> ReadWriteStore for &BS {
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

    fn bulk_write(
        &self,
        values: impl IntoIterator<Item = (impl Into<Vec<u8>>, impl Into<Vec<u8>>)>,
    ) -> Result<(), Error> {
        (*self).bulk_write(values)
    }

    fn bulk_delete<K>(&self, keys: &[K]) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
    {
        (*self).bulk_delete(keys)
    }
}

impl<BS: Store> Store for &BS
where
    BS: Sized,
{
    fn persistent(&self) -> &db_engine::Db {
        (*self).persistent()
    }

    fn rolling_by_epoch(
        &self,
        epoch: i64,
    ) -> SplitStore<ProxyStore<crate::db_engine::Db>, TrackingStore<crate::db_engine::Db>> {
        (*self).rolling_by_epoch(epoch)
    }

    fn rolling_by_epoch_raw(&self, epoch: i64) -> TrackingStore<crate::db_engine::Db> {
        (*self).rolling_by_epoch_raw(epoch)
    }

    fn rolling_stats(&self) -> String {
        (*self).rolling_stats()
    }

    fn rolling(&self) -> &RollingStore<crate::db_engine::Db> {
        (*self).rolling()
    }
}

/// Traits for collecting DB stats
pub trait DBStatistics {
    fn get_statistics(&self) -> Option<String> {
        None
    }
}

impl<T: DBStatistics> DBStatistics for Arc<T> {
    fn get_statistics(&self) -> Option<String> {
        self.as_ref().get_statistics()
    }
}

pub mod db_engine {
    use std::path::{Path, PathBuf};

    use crate::rolling::{ProxyStore, RollingStore};

    #[cfg(feature = "rocksdb")]
    pub type Db = crate::rocks::RocksDb;
    #[cfg(feature = "paritydb")]
    pub type Db = crate::parity_db::ParityDb;
    #[cfg(feature = "rocksdb")]
    pub type DbConfig = crate::rocks_config::RocksDbConfig;
    #[cfg(feature = "paritydb")]
    pub type DbConfig = crate::parity_db_config::ParityDbConfig;

    #[cfg(feature = "rocksdb")]
    const DIR_NAME: &str = "rocksdb";
    #[cfg(feature = "paritydb")]
    const DIR_NAME: &str = "paritydb";

    pub fn db_path(path: &Path) -> PathBuf {
        path.join(DIR_NAME)
    }

    #[cfg(feature = "rocksdb")]
    pub fn open_db(path: &std::path::Path, config: &DbConfig) -> anyhow::Result<Db> {
        crate::rocks::RocksDb::open(path, config).map_err(Into::into)
    }

    #[cfg(feature = "paritydb")]
    pub fn open_db(path: &std::path::Path, config: &DbConfig) -> anyhow::Result<Db> {
        crate::parity_db::ParityDb::open(path.into(), config).map_err(Into::into)
    }

    pub fn open_proxy_db(
        path: &std::path::Path,
        config: &DbConfig,
    ) -> anyhow::Result<ProxyStore<Db>> {
        let persistent = open_db(path, config)?;
        let rolling_path = path.join("..").join(format!("{DIR_NAME}_rolling"));
        let rolling = RollingStore::new(rolling_path);
        Ok(ProxyStore::new(persistent, rolling))
    }
}
