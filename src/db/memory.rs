// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

use crate::libp2p_bitswap::{BitswapStoreRead, BitswapStoreReadWrite};
use ahash::HashMap;
use anyhow::Result;
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use parking_lot::RwLock;

use super::SettingsStore;

type MemoryDbInner = Arc<RwLock<HashMap<Vec<u8>, Vec<u8>>>>;

/// A thread-safe `HashMap` wrapper, acting as a memory-backed blockstore.
#[derive(Debug, Default, Clone)]
pub struct MemoryDB(MemoryDbInner);

impl SettingsStore for MemoryDB {
    fn read_bin<K>(&self, key: K) -> anyhow::Result<Option<Vec<u8>>>
    where
        K: AsRef<str>,
    {
        Ok(self.0.read().get(key.as_ref().as_bytes()).cloned())
    }

    fn write_bin<K, V>(&self, key: K, value: V) -> anyhow::Result<()>
    where
        K: AsRef<str>,
        V: AsRef<[u8]>,
    {
        self.0
            .write()
            .insert(key.as_ref().as_bytes().to_vec(), value.as_ref().to_vec());
        Ok(())
    }

    fn exists<K>(&self, key: K) -> anyhow::Result<bool>
    where
        K: AsRef<str>,
    {
        Ok(self.0.read().contains_key(key.as_ref().as_bytes()))
    }
}

impl Blockstore for MemoryDB {
    fn get(&self, k: &Cid) -> Result<Option<Vec<u8>>> {
        Ok(self.0.read().get(&k.to_bytes()).cloned())
    }

    fn put_keyed(&self, k: &Cid, block: &[u8]) -> Result<()> {
        self.0.write().insert(k.to_bytes(), block.to_vec());
        Ok(())
    }
}

impl BitswapStoreRead for MemoryDB {
    fn contains(&self, cid: &Cid) -> Result<bool> {
        Ok(self.0.read().contains_key(&cid.to_bytes()))
    }

    fn get(&self, cid: &Cid) -> Result<Option<Vec<u8>>> {
        Blockstore::get(self, cid)
    }
}

impl BitswapStoreReadWrite for MemoryDB {
    type Params = libipld::DefaultParams;

    fn insert(&self, block: &libipld::Block<Self::Params>) -> Result<()> {
        self.put_keyed(block.cid(), block.data())
    }
}
