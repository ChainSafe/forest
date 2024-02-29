// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! The [`ManyCar`] block store is the union of `N` read-only CAR-backed block
//! stores and a single writable block store. Get requests are forwarded to each
//! store (including the writable store) and the first hit is returned. Write
//! requests are only forwarded to the writable store.
//!
//! A single z-frame cache is shared between all read-only stores.

use super::{AnyCar, ZstdFrameCache};
use crate::db::{MemoryDB, SettingsStore};
use crate::libp2p_bitswap::BitswapStoreReadWrite;
use crate::shim::clock::ChainEpoch;
use crate::utils::io::EitherMmapOrRandomAccessFile;
use crate::{blocks::Tipset, libp2p_bitswap::BitswapStoreRead};
use anyhow::Context as _;
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use parking_lot::{Mutex, RwLock};
use std::cmp::Ord;
use std::collections::BinaryHeap;
use std::{path::PathBuf, sync::Arc};
use tracing::debug;

struct WithHeaviestEpoch {
    pub car: AnyCar<Box<dyn super::RandomAccessFileReader>>,
    pub heaviest_epoch: ChainEpoch,
}

impl WithHeaviestEpoch {
    pub fn new(car: AnyCar<Box<dyn super::RandomAccessFileReader>>) -> anyhow::Result<Self> {
        let heaviest_epoch = car.heaviest_tipset()?.epoch();
        Ok(Self {
            car,
            heaviest_epoch,
        })
    }
}

impl Ord for WithHeaviestEpoch {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.heaviest_epoch.cmp(&other.heaviest_epoch)
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
        self.heaviest_epoch == other.heaviest_epoch
    }
}

pub struct ManyCar<WriterT = MemoryDB> {
    shared_cache: Arc<Mutex<ZstdFrameCache>>,
    read_only: RwLock<BinaryHeap<WithHeaviestEpoch>>,
    writer: WriterT,
}

impl<WriterT> ManyCar<WriterT> {
    pub fn new(writer: WriterT) -> Self {
        ManyCar {
            shared_cache: Arc::new(Mutex::new(ZstdFrameCache::default())),
            read_only: RwLock::new(BinaryHeap::default()),
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

    fn read_only<ReaderT: super::RandomAccessFileReader>(
        &self,
        any_car: AnyCar<ReaderT>,
    ) -> anyhow::Result<()> {
        let mut read_only = self.read_only.write();
        let key = read_only.len() as u64;

        let car = any_car
            .with_cache(self.shared_cache.clone(), key)
            .into_dyn();
        read_only
            .push(WithHeaviestEpoch::new(car).context("store doesn't have a heaviest tipset")?);

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
            let car = AnyCar::new(EitherMmapOrRandomAccessFile::open(&file)?)?;
            debug!("Loaded car DB at {}", file.display());

            self.read_only(car)?;
        }

        Ok(())
    }

    // TODO: update
    pub fn heaviest_tipset(&self) -> anyhow::Result<Tipset> {
        let tipsets = self
            .read_only
            .read()
            .iter()
            .map(|w| AnyCar::heaviest_tipset(&w.car))
            .collect::<anyhow::Result<Vec<_>>>()?;
        tipsets
            .into_iter()
            .max_by_key(Tipset::epoch)
            .context("ManyCar store doesn't have a heaviest tipset")
    }
}

impl<ReaderT: super::RandomAccessFileReader> From<AnyCar<ReaderT>> for ManyCar<MemoryDB> {
    fn from(any_car: AnyCar<ReaderT>) -> Self {
        ManyCar::default().with_read_only(any_car).unwrap()
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

impl<WriterT: BitswapStoreRead + Blockstore> BitswapStoreRead for ManyCar<WriterT> {
    fn contains(&self, cid: &Cid) -> anyhow::Result<bool> {
        Blockstore::has(self, cid)
    }

    fn get(&self, cid: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        Blockstore::get(self, cid)
    }
}

impl<WriterT: BitswapStoreReadWrite + Blockstore> BitswapStoreReadWrite for ManyCar<WriterT> {
    type Params = libipld::DefaultParams;

    fn insert(&self, block: &libipld::Block<Self::Params>) -> anyhow::Result<()> {
        Blockstore::put_keyed(self, block.cid(), block.data())
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

    // #[test]
    // fn many_car_idempotent() {
    //     let many = ManyCar::new(MemoryDB::default())
    //         .with_read_only(AnyCar::try_from(mainnet::DEFAULT_GENESIS).unwrap())
    //         .with_read_only(AnyCar::try_from(mainnet::DEFAULT_GENESIS).unwrap());
    //     assert_eq!(
    //         many.heaviest_tipset().unwrap(),
    //         AnyCar::try_from(mainnet::DEFAULT_GENESIS)
    //             .unwrap()
    //             .heaviest_tipset()
    //             .unwrap()
    //     );
    // }

    #[test]
    fn many_car_calibnet_heaviest() {
        let many = ManyCar::from(AnyCar::try_from(calibnet::DEFAULT_GENESIS).unwrap());
        let heaviest = many.heaviest_tipset().unwrap();
        assert_eq!(
            heaviest.min_ticket_block(),
            &heaviest.genesis(&many).unwrap()
        );
    }
}
