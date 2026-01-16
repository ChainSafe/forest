// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod blockstore_with_read_cache;
mod blockstore_with_write_buffer;
pub mod car;
mod memory;
pub mod parity_db;
pub mod parity_db_config;

pub mod gc;
pub mod ttl;
pub use blockstore_with_read_cache::*;
pub use blockstore_with_write_buffer::BlockstoreWithWriteBuffer;
pub use memory::MemoryDB;
mod db_mode;
mod either;
pub mod migration;
pub use either::Either;

use crate::blocks::TipsetKey;
use crate::rpc::eth::types::EthHash;
use anyhow::{Context as _, bail};
use cid::Cid;
pub use fvm_ipld_blockstore::{Blockstore, MemoryBlockstore};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::sync::Arc;

pub const CAR_DB_DIR_NAME: &str = "car_db";

pub mod setting_keys {
    /// Key used to store the heaviest tipset in the settings store. This is expected to be a [`crate::blocks::TipsetKey`]s
    pub const HEAD_KEY: &str = "head";
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
    #[allow(dead_code)]
    fn setting_keys(&self) -> anyhow::Result<Vec<String>>;
}

impl<T: SettingsStore> SettingsStore for Arc<T> {
    fn read_bin(&self, key: &str) -> anyhow::Result<Option<Vec<u8>>> {
        SettingsStore::read_bin(self.as_ref(), key)
    }

    fn write_bin(&self, key: &str, value: &[u8]) -> anyhow::Result<()> {
        SettingsStore::write_bin(self.as_ref(), key, value)
    }

    fn exists(&self, key: &str) -> anyhow::Result<bool> {
        SettingsStore::exists(self.as_ref(), key)
    }

    fn setting_keys(&self) -> anyhow::Result<Vec<String>> {
        SettingsStore::setting_keys(self.as_ref())
    }
}

/// Extension trait for the [`SettingsStore`] trait. It is implemented for all types that implement
/// [`SettingsStore`].
/// It provides methods for writing and reading any serializable object from the store.
pub trait SettingsStoreExt {
    fn read_obj<V: DeserializeOwned>(&self, key: &str) -> anyhow::Result<Option<V>>;
    fn write_obj<V: Serialize>(&self, key: &str, value: &V) -> anyhow::Result<()>;

    #[allow(dead_code)]
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
            .with_context(|| format!("Key {key} not found"))
            .and_then(|bytes| serde_json::from_slice(&bytes).map_err(Into::into))
    }
}

/// Interface used to store and retrieve Ethereum mappings from the database.
/// To store IPLD blocks, use the `BlockStore` trait.
pub trait EthMappingsStore {
    /// Reads binary field from the `EthMappings` store. This should be used for
    /// non-serializable data. For serializable data, use [`EthMappingsStoreExt::read_obj`].
    fn read_bin(&self, key: &EthHash) -> anyhow::Result<Option<Vec<u8>>>;

    /// Writes binary field to the `EthMappings` store. This should be used for
    /// non-serializable data. For serializable data, use [`EthMappingsStoreExt::write_obj`].
    fn write_bin(&self, key: &EthHash, value: &[u8]) -> anyhow::Result<()>;

    /// Returns `Ok(true)` if key exists in store.
    #[allow(dead_code)]
    fn exists(&self, key: &EthHash) -> anyhow::Result<bool>;

    /// Returns all message CIDs with their timestamp.
    fn get_message_cids(&self) -> anyhow::Result<Vec<(Cid, u64)>>;

    /// Deletes `keys` if keys exist in store.
    fn delete(&self, keys: Vec<EthHash>) -> anyhow::Result<()>;
}

impl<T: EthMappingsStore> EthMappingsStore for Arc<T> {
    fn read_bin(&self, key: &EthHash) -> anyhow::Result<Option<Vec<u8>>> {
        EthMappingsStore::read_bin(self.as_ref(), key)
    }

    fn write_bin(&self, key: &EthHash, value: &[u8]) -> anyhow::Result<()> {
        EthMappingsStore::write_bin(self.as_ref(), key, value)
    }

    fn exists(&self, key: &EthHash) -> anyhow::Result<bool> {
        EthMappingsStore::exists(self.as_ref(), key)
    }

    fn get_message_cids(&self) -> anyhow::Result<Vec<(Cid, u64)>> {
        EthMappingsStore::get_message_cids(self.as_ref())
    }

    fn delete(&self, keys: Vec<EthHash>) -> anyhow::Result<()> {
        EthMappingsStore::delete(self.as_ref(), keys)
    }
}

pub struct DummyStore {}

const INDEXER_ERROR: &str =
    "indexer disabled, enable with chain_indexer.enable_indexer / FOREST_CHAIN_INDEXER_ENABLED";

impl EthMappingsStore for DummyStore {
    fn read_bin(&self, _key: &EthHash) -> anyhow::Result<Option<Vec<u8>>> {
        bail!(INDEXER_ERROR)
    }

    fn write_bin(&self, _key: &EthHash, _value: &[u8]) -> anyhow::Result<()> {
        bail!(INDEXER_ERROR)
    }

    fn exists(&self, _key: &EthHash) -> anyhow::Result<bool> {
        bail!(INDEXER_ERROR)
    }

    fn get_message_cids(&self) -> anyhow::Result<Vec<(Cid, u64)>> {
        bail!(INDEXER_ERROR)
    }

    fn delete(&self, _keys: Vec<EthHash>) -> anyhow::Result<()> {
        bail!(INDEXER_ERROR)
    }
}

pub trait EthMappingsStoreExt {
    fn read_obj<V: DeserializeOwned>(&self, key: &EthHash) -> anyhow::Result<Option<V>>;
    fn write_obj<V: Serialize>(&self, key: &EthHash, value: &V) -> anyhow::Result<()>;
}

impl<T: ?Sized + EthMappingsStore> EthMappingsStoreExt for T {
    fn read_obj<V: DeserializeOwned>(&self, key: &EthHash) -> anyhow::Result<Option<V>> {
        match self.read_bin(key)? {
            Some(bytes) => Ok(Some(fvm_ipld_encoding::from_slice(&bytes)?)),
            None => Ok(None),
        }
    }

    fn write_obj<V: Serialize>(&self, key: &EthHash, value: &V) -> anyhow::Result<()> {
        self.write_bin(key, &fvm_ipld_encoding::to_vec(value)?)
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

/// A trait that allows for storing data that is not garbage collected.
pub trait PersistentStore: Blockstore {
    /// Puts a keyed block with pre-computed CID into the database.
    ///
    /// # Arguments
    ///
    /// * `k` - The key to be stored.
    /// * `block` - The block to be stored.
    fn put_keyed_persistent(&self, k: &Cid, block: &[u8]) -> anyhow::Result<()>;
}

impl PersistentStore for MemoryBlockstore {
    fn put_keyed_persistent(&self, k: &Cid, block: &[u8]) -> anyhow::Result<()> {
        self.put_keyed(k, block)
    }
}

impl<T: PersistentStore> PersistentStore for Arc<T> {
    fn put_keyed_persistent(&self, k: &Cid, block: &[u8]) -> anyhow::Result<()> {
        PersistentStore::put_keyed_persistent(self.as_ref(), k, block)
    }
}

impl<T: PersistentStore> PersistentStore for &Arc<T> {
    fn put_keyed_persistent(&self, k: &Cid, block: &[u8]) -> anyhow::Result<()> {
        PersistentStore::put_keyed_persistent(self.as_ref(), k, block)
    }
}

pub trait HeaviestTipsetKeyProvider {
    /// Returns the currently tracked heaviest tipset.
    fn heaviest_tipset_key(&self) -> anyhow::Result<TipsetKey>;

    /// Sets heaviest tipset.
    fn set_heaviest_tipset_key(&self, tsk: &TipsetKey) -> anyhow::Result<()>;
}

impl<T: HeaviestTipsetKeyProvider> HeaviestTipsetKeyProvider for Arc<T> {
    fn heaviest_tipset_key(&self) -> anyhow::Result<TipsetKey> {
        self.as_ref().heaviest_tipset_key()
    }

    fn set_heaviest_tipset_key(&self, tsk: &TipsetKey) -> anyhow::Result<()> {
        self.as_ref().set_heaviest_tipset_key(tsk)
    }
}

pub trait BlockstoreWriteOpsSubscribable {
    fn subscribe_write_ops(&self) -> tokio::sync::broadcast::Receiver<(Cid, Vec<u8>)>;

    fn unsubscribe_write_ops(&self);
}

impl<T: BlockstoreWriteOpsSubscribable> BlockstoreWriteOpsSubscribable for Arc<T> {
    fn subscribe_write_ops(&self) -> tokio::sync::broadcast::Receiver<(Cid, Vec<u8>)> {
        self.as_ref().subscribe_write_ops()
    }

    fn unsubscribe_write_ops(&self) {
        self.as_ref().unsubscribe_write_ops()
    }
}

pub mod db_engine {
    use std::path::{Path, PathBuf};

    use super::db_mode::choose_db;

    pub type Db = crate::db::parity_db::ParityDb;
    pub type DbConfig = crate::db::parity_db_config::ParityDbConfig;

    /// Returns the path to the database directory to be used by the daemon.
    pub fn db_root(chain_data_root: &Path) -> anyhow::Result<PathBuf> {
        choose_db(chain_data_root)
    }

    pub fn open_db(path: PathBuf, config: &DbConfig) -> anyhow::Result<Db> {
        Db::open(path, config)
    }
}

#[cfg(test)]
mod tests {
    pub mod db_utils;
    mod mem_test;
    mod parity_test;
    pub mod subtests;
}
