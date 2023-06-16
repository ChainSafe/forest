// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

use crate::libp2p_bitswap::{BitswapStoreRead, BitswapStoreReadWrite};
use ahash::HashMap;
use anyhow::Result;
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use parking_lot::RwLock;

use super::{Error, Store};

/// A thread-safe `HashMap` wrapper.
#[derive(Debug, Default, Clone)]
pub struct MemoryDB {
    db: Arc<RwLock<HashMap<Vec<u8>, Vec<u8>>>>,
}

impl Store for MemoryDB {
    fn write<K, V>(&self, key: K, value: V) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        self.db
            .write()
            .insert(key.as_ref().to_vec(), value.as_ref().to_vec());
        Ok(())
    }

    fn read<K>(&self, key: K) -> Result<Option<Vec<u8>>, Error>
    where
        K: AsRef<[u8]>,
    {
        Ok(self.db.read().get(key.as_ref()).cloned())
    }

    fn exists<K>(&self, key: K) -> Result<bool, Error>
    where
        K: AsRef<[u8]>,
    {
        Ok(self.db.read().contains_key(key.as_ref()))
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

impl BitswapStoreRead for MemoryDB {
    fn contains(&self, cid: &Cid) -> Result<bool> {
        Ok(self.exists(cid.to_bytes())?)
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
