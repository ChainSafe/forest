// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod memory;
mod metrics;
pub mod parity_db;
pub mod parity_db_config;
pub use memory::MemoryDB;
pub mod car;

pub mod rolling;

/// Interface used to store and retrieve settings from the database.
/// To store IPLD blocks, use the `BlockStore` trait.
pub trait SettingsStore {
    /// Reads binary field from the Settings store. This should be used for
    /// non-serializable data. For serializable data, use [`SettingsStore::read_obj`].
    fn read_bin<K>(&self, key: K) -> anyhow::Result<Option<Vec<u8>>>
    where
        K: AsRef<str>;

    /// Writes binary field to the Settings store. This should be used for
    /// non-serializable data. For serializable data, use [`SettingsStore::write_obj`].
    fn write_bin<K, V>(&self, key: K, value: V) -> anyhow::Result<()>
    where
        K: AsRef<str>,
        V: AsRef<[u8]>;

    fn read_obj<K, T>(&self, key: K) -> anyhow::Result<Option<T>>
    where
        K: AsRef<str>,
        T: serde::de::DeserializeOwned,
    {
        match self.read_bin(key)? {
            Some(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            None => Ok(None),
        }
    }

    fn write_obj<K, T>(&self, key: K, value: &T) -> anyhow::Result<()>
    where
        K: AsRef<str>,
        T: serde::Serialize,
    {
        self.write_bin(key, serde_json::to_vec(value)?)
    }

    /// Returns `Ok(true)` if key exists in store
    fn exists<K>(&self, key: K) -> anyhow::Result<bool>
    where
        K: AsRef<str>;
}

/// Traits for collecting DB stats
pub trait DBStatistics {
    fn get_statistics(&self) -> Option<String> {
        None
    }
}

impl<DB: DBStatistics> DBStatistics for std::sync::Arc<DB> {
    fn get_statistics(&self) -> Option<String> {
        self.as_ref().get_statistics()
    }
}

pub mod db_engine {
    use std::path::{Path, PathBuf};

    use crate::db::rolling::*;

    pub type Db = crate::db::parity_db::ParityDb;
    pub type DbConfig = crate::db::parity_db_config::ParityDbConfig;
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
