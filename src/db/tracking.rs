// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::libp2p_bitswap::{BitswapStoreRead, BitswapStoreReadWrite};
use fvm_ipld_blockstore::Blockstore;
use parking_lot::RwLock;

pub struct TrackingStore<T> {
    inner: T,
    pub tracked: Arc<RwLock<Option<MemoryDB>>>,
}

impl<T> TrackingStore<T> {
    pub fn new(inner: T) -> Self {
        Self {
            inner,
            tracked: Arc::new(RwLock::new(None)),
        }
    }

    pub fn inner(&self) -> &T {
        &self.inner
    }

    pub fn start_tracking(&self) {
        self.tracked.write().replace(MemoryDB::default());
    }

    pub fn stop_tracking(&self) -> Option<MemoryDB> {
        self.tracked.write().take()
    }
}

impl<T: Blockstore> Blockstore for TrackingStore<T> {
    fn get(&self, k: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        let result = self.inner.get(k)?;
        if let (Some(tracked), Some(v)) = (self.tracked.read().as_ref(), &result) {
            tracked.put_keyed(k, v.as_slice())?;
        }
        Ok(result)
    }

    fn put_keyed(&self, k: &Cid, block: &[u8]) -> anyhow::Result<()> {
        self.inner.put_keyed(k, block)
    }
}

impl<T: SettingsStore> SettingsStore for TrackingStore<T> {
    fn read_bin(&self, key: &str) -> anyhow::Result<Option<Vec<u8>>> {
        let result = self.inner.read_bin(key)?;
        if let (Some(tracked), Some(v)) = (self.tracked.read().as_ref(), &result) {
            tracing::info!("tracing read_bin: {key}");
            SettingsStore::write_bin(tracked, key, v.as_slice())?;
        }
        Ok(result)
    }

    fn write_bin(&self, key: &str, value: &[u8]) -> anyhow::Result<()> {
        self.inner.write_bin(key, value)
    }

    fn exists(&self, key: &str) -> anyhow::Result<bool> {
        let result = self.inner.read_bin(key)?;
        if let (Some(tracked), Some(v)) = (self.tracked.read().as_ref(), &result) {
            SettingsStore::write_bin(tracked, key, v.as_slice())?;
        }
        Ok(result.is_some())
    }

    fn setting_keys(&self) -> anyhow::Result<Vec<String>> {
        // HACKHACK: may need some care
        self.inner.setting_keys()
    }
}

impl<T: BitswapStoreRead> BitswapStoreRead for TrackingStore<T> {
    fn contains(&self, cid: &Cid) -> anyhow::Result<bool> {
        // HACKHACK: may need some care
        self.inner.contains(cid)
    }

    fn get(&self, cid: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        // HACKHACK: may need some care
        self.inner.get(cid)
    }
}

impl<T: BitswapStoreReadWrite> BitswapStoreReadWrite for TrackingStore<T> {
    type Params = <T as BitswapStoreReadWrite>::Params;

    fn insert(&self, block: &libipld::Block<Self::Params>) -> anyhow::Result<()> {
        self.inner.insert(block)
    }
}

impl<T: EthMappingsStore> EthMappingsStore for TrackingStore<T> {
    fn read_bin(&self, key: &EthHash) -> anyhow::Result<Option<Vec<u8>>> {
        // HACKHACK: may need some care
        self.inner.read_bin(key)
    }

    fn write_bin(&self, key: &EthHash, value: &[u8]) -> anyhow::Result<()> {
        self.inner.write_bin(key, value)
    }

    fn exists(&self, key: &EthHash) -> anyhow::Result<bool> {
        // HACKHACK: may need some care
        self.inner.exists(key)
    }

    fn get_message_cids(&self) -> anyhow::Result<Vec<(Cid, u64)>> {
        // HACKHACK: may need some care
        self.inner.get_message_cids()
    }

    fn delete(&self, keys: Vec<EthHash>) -> anyhow::Result<()> {
        self.inner.delete(keys)
    }
}
