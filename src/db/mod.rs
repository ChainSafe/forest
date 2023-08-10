// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod memory;
mod metrics;
pub mod parity_db;
pub mod parity_db_config;

pub use memory::MemoryDB;
use serde::de::DeserializeOwned;
use serde::Serialize;
pub mod car;

pub mod rolling;

pub mod setting_keys {
    /// Key used to store the heaviest tipset in the settings store.
    pub const HEAD_KEY: &str = "head";
    /// Estimated number of IPLD records in the database.
    pub const ESTIMATED_RECORDS_KEY: &str = "estimated_reachable_records";
    /// Key used to store the memory pool configuration in the settings store.
    pub const MPOOL_CONFIG_KEY: &str = "/mpool/config";
}

/// Interface used to store and retrieve settings from the database.
/// To store IPLD blocks, use the `BlockStore` trait.
pub trait SettingsStore {
    /// Reads binary field from the Settings store. This should be used for
    /// non-serializable data. For serializable data, use [`SettingsStoreExt::read_obj`].
    fn read_bin(&self, key: &str) -> anyhow::Result<Option<Vec<u8>>>;

    /// Writes binary field to the Settings store. This should be used for
    /// non-serializable data. For serializable data, use [`SettingsStoreExt::write_obj`].
    fn write_bin(&self, key: &str, value: &[u8]) -> anyhow::Result<()>;

    /// Returns `Ok(true)` if key exists in store.
    fn exists(&self, key: &str) -> anyhow::Result<bool>;

    /// Returns all setting keys.
    fn setting_keys(&self) -> anyhow::Result<Vec<String>>;
}

/// Extension trait for the [`SettingsStore`] trait. It is implemented for all types that implement
/// [`SettingsStore`].
/// It provides methods for writing and reading any serializable object from the store.
pub trait SettingsStoreExt {
    fn read_obj<V: DeserializeOwned>(&self, key: &str) -> anyhow::Result<Option<V>>;
    fn write_obj<V: Serialize>(&self, key: &str, value: &V) -> anyhow::Result<()>;

    /// Same as [`SettingsStoreExt::read_obj`], but returns an error if the key does not exist.
    fn require_obj<V: DeserializeOwned>(&self, key: &str) -> anyhow::Result<V>;
}

impl<T: ?Sized + SettingsStore> SettingsStoreExt for T {
    fn read_obj<V: DeserializeOwned>(&self, key: &str) -> anyhow::Result<Option<V>> {
        match self.read_bin(key)? {
            Some(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            None => Ok(None),
        }
    }

    fn write_obj<V: Serialize>(&self, key: &str, value: &V) -> anyhow::Result<()> {
        self.write_bin(key, &serde_json::to_vec(value)?)
    }

    fn require_obj<V: DeserializeOwned>(&self, key: &str) -> anyhow::Result<V> {
        self.read_bin(key)?
            .ok_or_else(|| anyhow::anyhow!("Key {key} not found"))
            .and_then(|bytes| serde_json::from_slice(&bytes).map_err(Into::into))
    }
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
