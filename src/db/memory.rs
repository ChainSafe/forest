// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::cid_collections::CidHashSet;
use crate::db::{GarbageCollectable, PersistentStore};
use crate::libp2p_bitswap::{BitswapStoreRead, BitswapStoreReadWrite};
use crate::rpc::eth::types::EthHash;
use crate::utils::multihash::prelude::*;
use ahash::HashMap;
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use itertools::Itertools;
use parking_lot::RwLock;

use super::{EthMappingsStore, SettingsStore};

#[derive(Debug, Default)]
pub struct MemoryDB {
    blockchain_db: RwLock<HashMap<Vec<u8>, Vec<u8>>>,
    blockchain_persistent_db: RwLock<HashMap<Vec<u8>, Vec<u8>>>,
    settings_db: RwLock<HashMap<String, Vec<u8>>>,
    eth_mappings_db: RwLock<HashMap<EthHash, Vec<u8>>>,
}

impl GarbageCollectable<CidHashSet> for MemoryDB {
    fn get_keys(&self) -> anyhow::Result<CidHashSet> {
        let mut set = CidHashSet::new();
        for key in self.blockchain_db.read().keys() {
            let cid = Cid::try_from(key.as_slice())?;
            set.insert(cid);
        }
        Ok(set)
    }

    fn remove_keys(&self, keys: CidHashSet) -> anyhow::Result<u32> {
        let mut db = self.blockchain_db.write();
        let mut deleted = 0;
        db.retain(|key, _| {
            let cid = Cid::try_from(key.as_slice());
            match cid {
                Ok(cid) => {
                    let retain = !keys.contains(&cid);
                    if !retain {
                        deleted += 1;
                    }
                    retain
                }
                _ => true,
            }
        });
        Ok(deleted)
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

impl EthMappingsStore for MemoryDB {
    fn read_bin(&self, key: &EthHash) -> anyhow::Result<Option<Vec<u8>>> {
        Ok(self.eth_mappings_db.read().get(key).cloned())
    }

    fn write_bin(&self, key: &EthHash, value: &[u8]) -> anyhow::Result<()> {
        self.eth_mappings_db
            .write()
            .insert(key.to_owned(), value.to_vec());
        Ok(())
    }

    fn exists(&self, key: &EthHash) -> anyhow::Result<bool> {
        Ok(self.eth_mappings_db.read().contains_key(key))
    }

    fn get_message_cids(&self) -> anyhow::Result<Vec<(Cid, u64)>> {
        let cids = self
            .eth_mappings_db
            .read()
            .iter()
            .filter_map(|(_, value)| fvm_ipld_encoding::from_slice::<(Cid, u64)>(value).ok())
            .collect();

        Ok(cids)
    }

    fn delete(&self, keys: Vec<EthHash>) -> anyhow::Result<()> {
        let mut lock = self.eth_mappings_db.write();
        for hash in keys.iter() {
            lock.remove(hash);
        }
        Ok(())
    }
}

impl Blockstore for MemoryDB {
    fn get(&self, k: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        Ok(self
            .blockchain_db
            .read()
            .get(&k.to_bytes())
            .cloned()
            .or(self
                .blockchain_persistent_db
                .read()
                .get(&k.to_bytes())
                .cloned()))
    }

    fn put_keyed(&self, k: &Cid, block: &[u8]) -> anyhow::Result<()> {
        self.blockchain_db
            .write()
            .insert(k.to_bytes(), block.to_vec());
        Ok(())
    }
}

impl PersistentStore for MemoryDB {
    fn put_keyed_persistent(&self, k: &Cid, block: &[u8]) -> anyhow::Result<()> {
        self.blockchain_persistent_db
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
    type Hashes = MultihashCode;

    fn insert(&self, block: &crate::libp2p_bitswap::Block64<Self::Hashes>) -> anyhow::Result<()> {
        self.put_keyed(block.cid(), block.data())
    }
}
