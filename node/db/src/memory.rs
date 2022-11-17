// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::utils::bitswap_missing_blocks;

use super::{Error, Store};
use anyhow::Result;
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use libp2p_bitswap::BitswapStore;
use parking_lot::RwLock;
use std::collections::{hash_map::DefaultHasher, HashMap};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

/// A thread-safe `HashMap` wrapper.
#[derive(Debug, Default, Clone)]
pub struct MemoryDB {
    db: Arc<RwLock<HashMap<u64, Vec<u8>>>>,
}

impl MemoryDB {
    fn db_index<K>(key: K) -> u64
    where
        K: AsRef<[u8]>,
    {
        let mut hasher = DefaultHasher::new();
        key.as_ref().hash::<DefaultHasher>(&mut hasher);
        hasher.finish()
    }
}

impl Store for MemoryDB {
    fn write<K, V>(&self, key: K, value: V) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        self.db
            .write()
            .insert(Self::db_index(key), value.as_ref().to_vec());
        Ok(())
    }

    fn delete<K>(&self, key: K) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
    {
        self.db.write().remove(&Self::db_index(key));
        Ok(())
    }

    fn read<K>(&self, key: K) -> Result<Option<Vec<u8>>, Error>
    where
        K: AsRef<[u8]>,
    {
        Ok(self.db.read().get(&Self::db_index(key)).cloned())
    }

    fn exists<K>(&self, key: K) -> Result<bool, Error>
    where
        K: AsRef<[u8]>,
    {
        Ok(self.db.read().contains_key(&Self::db_index(key)))
    }
}

impl Blockstore for MemoryDB {
    fn get(&self, k: &Cid) -> Result<Option<Vec<u8>>> {
        self.read(k.to_bytes()).map_err(|e| e.into())
    }

    fn put_keyed(&self, k: &Cid, block: &[u8]) -> Result<()> {
        self.write(k.to_bytes(), block).map_err(|e| e.into())
    }
}

impl BitswapStore for MemoryDB {
    type Params = libipld::DefaultParams;

    fn contains(&mut self, cid: &Cid) -> Result<bool> {
        Ok(self.exists(cid.to_bytes())?)
    }

    fn get(&mut self, cid: &Cid) -> Result<Option<Vec<u8>>> {
        Blockstore::get(self, cid)
    }

    fn insert(&mut self, block: &libipld::Block<Self::Params>) -> Result<()> {
        self.put_keyed(block.cid(), block.data())
    }

    fn missing_blocks(&mut self, cid: &Cid) -> Result<Vec<Cid>> {
        bitswap_missing_blocks::<_, Self::Params>(self, cid)
    }
}
