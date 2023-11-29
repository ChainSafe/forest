// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::db::{truncated_hash, GarbageCollectable};
use crate::libp2p_bitswap::{BitswapStoreRead, BitswapStoreReadWrite};
use ahash::{HashMap, HashSet, HashSetExt};
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use itertools::Itertools;
use parking_lot::RwLock;

use super::SettingsStore;

#[derive(Debug, Default)]
pub struct MemoryDB {
    blockchain_db: RwLock<HashMap<Vec<u8>, Vec<u8>>>,
    settings_db: RwLock<HashMap<String, Vec<u8>>>,
}

impl GarbageCollectable for MemoryDB {
    fn get_keys(&self) -> anyhow::Result<HashSet<u32>> {
        let mut set = HashSet::with_capacity(self.blockchain_db.read().len());
        for key in self.blockchain_db.read().keys() {
            let cid = Cid::try_from(key.as_slice())?;
            set.insert(truncated_hash(cid.hash()));
        }
        Ok(set)
    }

    fn remove_keys(&self, keys: HashSet<u32>) -> anyhow::Result<()> {
        let mut db = self.blockchain_db.write();
        db.retain(|key, _| {
            let cid = Cid::try_from(key.as_slice());
            match cid {
                Ok(cid) => !keys.contains(&truncated_hash(cid.hash())),
                _ => true,
            }
        });
        Ok(())
    }
}

impl SettingsStore for MemoryDB {
    fn read_bin(&self, key: &str) -> anyhow::Result<Option<Vec<u8>>> {
        Ok(self.settings_db.read().get(key).cloned())
    }

    fn write_bin(&self, key: &str, value: &[u8]) -> anyhow::Result<()> {
        self.settings_db
            .write()
            .insert(key.to_owned(), value.to_vec());
        Ok(())
    }

    fn exists(&self, key: &str) -> anyhow::Result<bool> {
        Ok(self.settings_db.read().contains_key(key))
    }

    fn setting_keys(&self) -> anyhow::Result<Vec<String>> {
        Ok(self.settings_db.read().keys().cloned().collect_vec())
    }
}

impl Blockstore for MemoryDB {
    fn get(&self, k: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        Ok(self.blockchain_db.read().get(&k.to_bytes()).cloned())
    }

    fn put_keyed(&self, k: &Cid, block: &[u8]) -> anyhow::Result<()> {
        self.blockchain_db
            .write()
            .insert(k.to_bytes(), block.to_vec());
        Ok(())
    }
}

impl BitswapStoreRead for MemoryDB {
    fn contains(&self, cid: &Cid) -> anyhow::Result<bool> {
        Ok(self.blockchain_db.read().contains_key(&cid.to_bytes()))
    }

    fn get(&self, cid: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        Blockstore::get(self, cid)
    }
}

impl BitswapStoreReadWrite for MemoryDB {
    type Params = libipld::DefaultParams;

    fn insert(&self, block: &libipld::Block<Self::Params>) -> anyhow::Result<()> {
        self.put_keyed(block.cid(), block.data())
    }
}
