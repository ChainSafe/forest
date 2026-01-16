// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! The [`ManyCar`] block store is the union of `N` read-only CAR-backed block
//! stores and a single writable block store. Get requests are forwarded to each
//! store (including the writable store) and the first hit is returned. Write
//! requests are only forwarded to the writable store.
//!
//! A single z-frame cache is shared between all read-only stores.

use super::{AnyCar, ZstdFrameCache};
use crate::blocks::TipsetKey;
use crate::db::{
    BlockstoreWriteOpsSubscribable, EthMappingsStore, MemoryDB, PersistentStore, SettingsStore,
    SettingsStoreExt,
};
use crate::libp2p_bitswap::BitswapStoreReadWrite;
use crate::rpc::eth::types::EthHash;
use crate::shim::clock::ChainEpoch;
use crate::utils::io::EitherMmapOrRandomAccessFile;
use crate::utils::multihash::prelude::*;
use crate::{blocks::Tipset, libp2p_bitswap::BitswapStoreRead};
use anyhow::Context as _;
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use parking_lot::RwLock;
use std::cmp::Ord;
use std::collections::BinaryHeap;
use std::{path::PathBuf, sync::Arc};

struct WithHeaviestEpoch {
    pub car: AnyCar<Box<dyn super::RandomAccessFileReader>>,
    epoch: ChainEpoch,
}

impl WithHeaviestEpoch {
    pub fn new(car: AnyCar<Box<dyn super::RandomAccessFileReader>>) -> anyhow::Result<Self> {
        let epoch = car
            .heaviest_tipset()
            .context("store doesn't have a heaviest tipset")?
            .epoch();
        Ok(Self { car, epoch })
    }
}

impl Ord for WithHeaviestEpoch {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.epoch.cmp(&other.epoch)
    }
}

impl Eq for WithHeaviestEpoch {}

impl PartialOrd for WithHeaviestEpoch {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for WithHeaviestEpoch {
    fn eq(&self, other: &Self) -> bool {
        self.epoch == other.epoch
    }
}

pub struct ManyCar<WriterT = MemoryDB> {
    shared_cache: Arc<ZstdFrameCache>,
    read_only: Arc<RwLock<BinaryHeap<WithHeaviestEpoch>>>,
    writer: WriterT,
}

impl<WriterT> ManyCar<WriterT> {
    pub fn new(writer: WriterT) -> Self {
        ManyCar {
            shared_cache: Arc::new(ZstdFrameCache::default()),
            read_only: Arc::new(RwLock::new(BinaryHeap::default())),
            writer,
        }
    }

    pub fn writer(&self) -> &WriterT {
        &self.writer
    }
}

impl<WriterT: Default> Default for ManyCar<WriterT> {
    fn default() -> Self {
        Self::new(Default::default())
    }
}

impl<WriterT> ManyCar<WriterT> {
    pub fn with_read_only<ReaderT: super::RandomAccessFileReader>(
        self,
        any_car: AnyCar<ReaderT>,
    ) -> anyhow::Result<Self> {
        self.read_only(any_car)?;
        Ok(self)
    }

    pub fn read_only<ReaderT: super::RandomAccessFileReader>(
        &self,
        any_car: AnyCar<ReaderT>,
    ) -> anyhow::Result<()> {
        let mut read_only = self.read_only.write();
        let key = read_only.len() as u64;

        read_only.push(WithHeaviestEpoch::new(
            any_car
                .with_cache(self.shared_cache.clone(), key)
                .into_dyn(),
        )?);

        Ok(())
    }

    pub fn with_read_only_files(
        self,
        files: impl Iterator<Item = PathBuf>,
    ) -> anyhow::Result<Self> {
        self.read_only_files(files)?;
        Ok(self)
    }

    pub fn read_only_files(&self, files: impl Iterator<Item = PathBuf>) -> anyhow::Result<()> {
        for file in files {
            self.read_only(AnyCar::new(EitherMmapOrRandomAccessFile::open(file)?)?)?;
        }

        Ok(())
    }

    pub fn heaviest_tipset_key(&self) -> anyhow::Result<TipsetKey> {
        self.read_only
            .read()
            .peek()
            .map(|w| AnyCar::heaviest_tipset_key(&w.car))
            .context("ManyCar store doesn't have a heaviest tipset key")
    }

    pub fn heaviest_tipset(&self) -> anyhow::Result<Tipset> {
        self.read_only
            .read()
            .peek()
            .map(|w| AnyCar::heaviest_tipset(&w.car))
            .context("ManyCar store doesn't have a heaviest tipset")?
    }

    /// Number of read-only `CAR`s
    pub fn len(&self) -> usize {
        self.read_only.read().len()
    }
}

impl<ReaderT: super::RandomAccessFileReader> TryFrom<AnyCar<ReaderT>> for ManyCar<MemoryDB> {
    type Error = anyhow::Error;
    fn try_from(any_car: AnyCar<ReaderT>) -> anyhow::Result<Self> {
        ManyCar::default().with_read_only(any_car)
    }
}

impl TryFrom<Vec<PathBuf>> for ManyCar<MemoryDB> {
    type Error = anyhow::Error;
    fn try_from(files: Vec<PathBuf>) -> anyhow::Result<Self> {
        ManyCar::default().with_read_only_files(files.into_iter())
    }
}

impl<WriterT: Blockstore> Blockstore for ManyCar<WriterT> {
    fn get(&self, k: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        // Theoretically it should be easily parallelizable with `rayon`.
        // In practice, there is a massive performance loss when providing
        // more than a single reader.
        if let Ok(Some(value)) = self.writer.get(k) {
            return Ok(Some(value));
        }
        for reader in self.read_only.read().iter() {
            if let Some(val) = reader.car.get(k)? {
                return Ok(Some(val));
            }
        }
        Ok(None)
    }

    fn put_keyed(&self, k: &Cid, block: &[u8]) -> anyhow::Result<()> {
        self.writer.put_keyed(k, block)
    }
}

impl<WriterT: PersistentStore> PersistentStore for ManyCar<WriterT> {
    fn put_keyed_persistent(&self, k: &Cid, block: &[u8]) -> anyhow::Result<()> {
        self.writer.put_keyed_persistent(k, block)
    }
}

impl<WriterT: BitswapStoreRead + Blockstore> BitswapStoreRead for ManyCar<WriterT> {
    fn contains(&self, cid: &Cid) -> anyhow::Result<bool> {
        Blockstore::has(self, cid)
    }

    fn get(&self, cid: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        Blockstore::get(self, cid)
    }
}

impl<WriterT: BitswapStoreReadWrite + Blockstore> BitswapStoreReadWrite for ManyCar<WriterT> {
    type Hashes = MultihashCode;

    fn insert(&self, block: &crate::libp2p_bitswap::Block64<Self::Hashes>) -> anyhow::Result<()> {
        self.put_keyed(block.cid(), block.data())
    }
}

impl<WriterT: SettingsStore> SettingsStore for ManyCar<WriterT> {
    fn read_bin(&self, key: &str) -> anyhow::Result<Option<Vec<u8>>> {
        SettingsStore::read_bin(self.writer(), key)
    }

    fn write_bin(&self, key: &str, value: &[u8]) -> anyhow::Result<()> {
        SettingsStore::write_bin(self.writer(), key, value)
    }

    fn exists(&self, key: &str) -> anyhow::Result<bool> {
        SettingsStore::exists(self.writer(), key)
    }

    fn setting_keys(&self) -> anyhow::Result<Vec<String>> {
        SettingsStore::setting_keys(self.writer())
    }
}

impl<WriterT: EthMappingsStore> EthMappingsStore for ManyCar<WriterT> {
    fn read_bin(&self, key: &EthHash) -> anyhow::Result<Option<Vec<u8>>> {
        EthMappingsStore::read_bin(self.writer(), key)
    }

    fn write_bin(&self, key: &EthHash, value: &[u8]) -> anyhow::Result<()> {
        EthMappingsStore::write_bin(self.writer(), key, value)
    }

    fn exists(&self, key: &EthHash) -> anyhow::Result<bool> {
        EthMappingsStore::exists(self.writer(), key)
    }

    fn get_message_cids(&self) -> anyhow::Result<Vec<(Cid, u64)>> {
        EthMappingsStore::get_message_cids(self.writer())
    }

    fn delete(&self, keys: Vec<EthHash>) -> anyhow::Result<()> {
        EthMappingsStore::delete(self.writer(), keys)
    }
}

impl<T: Blockstore + SettingsStore> super::super::HeaviestTipsetKeyProvider for ManyCar<T> {
    fn heaviest_tipset_key(&self) -> anyhow::Result<TipsetKey> {
        match SettingsStoreExt::read_obj::<TipsetKey>(self, crate::db::setting_keys::HEAD_KEY)? {
            Some(tsk) => Ok(tsk),
            None => self.heaviest_tipset_key(),
        }
    }

    fn set_heaviest_tipset_key(&self, tsk: &TipsetKey) -> anyhow::Result<()> {
        SettingsStoreExt::write_obj(self, crate::db::setting_keys::HEAD_KEY, tsk)
    }
}

impl<WriterT: BlockstoreWriteOpsSubscribable> BlockstoreWriteOpsSubscribable for ManyCar<WriterT> {
    fn subscribe_write_ops(&self) -> tokio::sync::broadcast::Receiver<(Cid, Vec<u8>)> {
        self.writer().subscribe_write_ops()
    }

    fn unsubscribe_write_ops(&self) {
        self.writer().unsubscribe_write_ops()
    }
}

#[cfg(test)]
mod tests {
    use super::super::AnyCar;
    use super::*;
    use crate::networks::{calibnet, mainnet};

    #[test]
    fn many_car_empty() {
        let many = ManyCar::new(MemoryDB::default());
        assert!(many.heaviest_tipset().is_err());
    }

    #[test]
    fn many_car_idempotent() {
        let many = ManyCar::new(MemoryDB::default())
            .with_read_only(AnyCar::try_from(mainnet::DEFAULT_GENESIS).unwrap())
            .unwrap()
            .with_read_only(AnyCar::try_from(mainnet::DEFAULT_GENESIS).unwrap())
            .unwrap();
        assert_eq!(
            many.heaviest_tipset().unwrap(),
            AnyCar::try_from(mainnet::DEFAULT_GENESIS)
                .unwrap()
                .heaviest_tipset()
                .unwrap()
        );
    }

    #[test]
    fn many_car_calibnet_heaviest() {
        let many = ManyCar::try_from(AnyCar::try_from(calibnet::DEFAULT_GENESIS).unwrap()).unwrap();
        let heaviest = many.heaviest_tipset().unwrap();
        assert_eq!(
            heaviest.min_ticket_block(),
            &heaviest.genesis(&many).unwrap()
        );
    }
}
