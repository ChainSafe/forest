// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{EthMappingsStore, SettingsStore, SettingsStoreExt};
use crate::blocks::TipsetKey;
use crate::db::PersistentStore;
use crate::libp2p_bitswap::{BitswapStoreRead, BitswapStoreReadWrite};
use crate::rpc::eth::types::EthHash;
use crate::utils::db::car_stream::CarBlock;
use crate::utils::multihash::prelude::*;
use ahash::HashMap;
use anyhow::Context as _;
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use itertools::Itertools;
use parking_lot::RwLock;

#[derive(Debug, Default)]
pub struct MemoryDB {
    blockchain_db: RwLock<HashMap<Cid, Vec<u8>>>,
    blockchain_persistent_db: RwLock<HashMap<Cid, Vec<u8>>>,
    settings_db: RwLock<HashMap<String, Vec<u8>>>,
    pub eth_mappings_db: RwLock<HashMap<EthHash, Vec<u8>>>,
}

impl MemoryDB {
    pub async fn export_forest_car<W: tokio::io::AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
    ) -> anyhow::Result<()> {
        let roots =
            SettingsStoreExt::read_obj::<TipsetKey>(self, crate::db::setting_keys::HEAD_KEY)?
                .context("chain head is not tracked and cannot be exported")?
                .into_cids();
        let blocks = {
            let blockchain_db = self.blockchain_db.read();
            let blockchain_persistent_db = self.blockchain_persistent_db.read();
            blockchain_db
                .iter()
                .chain(blockchain_persistent_db.iter())
                // Sort to make the result CAR deterministic
                .sorted_by_key(|&(&cid, _)| cid)
                .map(|(&cid, data)| {
                    anyhow::Ok(CarBlock {
                        cid,
                        data: data.clone(),
                    })
                })
                .collect_vec()
        };
        let frames =
            crate::db::car::forest::Encoder::compress_stream_default(futures::stream::iter(blocks));
        crate::db::car::forest::Encoder::write(writer, roots, frames).await
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
        Ok(self.blockchain_db.read().get(k).cloned().or(self
            .blockchain_persistent_db
            .read()
            .get(k)
            .cloned()))
    }

    fn put_keyed(&self, k: &Cid, block: &[u8]) -> anyhow::Result<()> {
        self.blockchain_db.write().insert(*k, block.to_vec());
        Ok(())
    }
}

impl PersistentStore for MemoryDB {
    fn put_keyed_persistent(&self, k: &Cid, block: &[u8]) -> anyhow::Result<()> {
        self.blockchain_persistent_db
            .write()
            .insert(*k, block.to_vec());
        Ok(())
    }
}

impl BitswapStoreRead for MemoryDB {
    fn contains(&self, cid: &Cid) -> anyhow::Result<bool> {
        Ok(self.blockchain_db.read().contains_key(cid))
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

impl super::HeaviestTipsetKeyProvider for MemoryDB {
    fn heaviest_tipset_key(&self) -> anyhow::Result<TipsetKey> {
        SettingsStoreExt::read_obj::<TipsetKey>(self, crate::db::setting_keys::HEAD_KEY)?
            .context("head key not found")
    }

    fn set_heaviest_tipset_key(&self, tsk: &TipsetKey) -> anyhow::Result<()> {
        SettingsStoreExt::write_obj(self, crate::db::setting_keys::HEAD_KEY, tsk)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{car::ForestCar, setting_keys::HEAD_KEY};
    use fvm_ipld_encoding::DAG_CBOR;
    use multihash_codetable::Code::Blake2b256;
    use nunny::vec as nonempty;

    #[tokio::test]
    async fn test_export_forest_car() {
        let db = MemoryDB::default();
        let record1 = b"non-persistent";
        let key1 = Cid::new_v1(DAG_CBOR, Blake2b256.digest(record1.as_slice()));
        db.put_keyed(&key1, record1.as_slice()).unwrap();

        let record2 = b"persistent";
        let key2 = Cid::new_v1(DAG_CBOR, Blake2b256.digest(record2.as_slice()));
        db.put_keyed_persistent(&key2, record2.as_slice()).unwrap();

        let mut car_db_bytes = vec![];
        assert!(
            db.export_forest_car(&mut car_db_bytes)
                .await
                .unwrap_err()
                .to_string()
                .contains("chain head is not tracked and cannot be exported")
        );

        db.write_obj(HEAD_KEY, &TipsetKey::from(nonempty![key1]))
            .unwrap();

        car_db_bytes.clear();
        db.export_forest_car(&mut car_db_bytes).await.unwrap();

        let car = ForestCar::new(car_db_bytes).unwrap();
        assert_eq!(car.head_tipset_key(), &nonempty![key1]);
        assert!(car.has(&key1).unwrap());
        assert!(car.has(&key2).unwrap());
    }
}
