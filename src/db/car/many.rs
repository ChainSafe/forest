// Copyright 2019-2023 ChainSafe Systems
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
use crate::utils::io::EitherMmapOrRandomAccessFile;
use crate::{blocks::Tipset, libp2p_bitswap::BitswapStoreRead};
use anyhow::Context as _;
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use parking_lot::{Mutex, RwLock};
use std::{io, path::PathBuf, sync::Arc};

pub struct ManyCar<WriterT = MemoryDB> {
    shared_cache: Arc<Mutex<ZstdFrameCache>>,
    read_only: RwLock<Vec<AnyCar<Box<dyn super::RandomAccessFileReader>>>>,
    writer: WriterT,
}

impl<WriterT> ManyCar<WriterT> {
    pub fn new(writer: WriterT) -> Self {
        ManyCar {
            shared_cache: Arc::new(Mutex::new(ZstdFrameCache::default())),
            read_only: RwLock::new(Vec::new()),
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
    ) -> Self {
        self.read_only(any_car);
        self
    }

    pub fn read_only<ReaderT: super::RandomAccessFileReader>(&self, any_car: AnyCar<ReaderT>) {
        let mut read_only = self.read_only.write();
        let key = read_only.len() as u64;
        read_only.push(
            any_car
                .with_cache(self.shared_cache.clone(), key)
                .into_dyn(),
        );
    }

    pub fn with_read_only_files(self, files: impl Iterator<Item = PathBuf>) -> io::Result<Self> {
        self.read_only_files(files)?;
        Ok(self)
    }

    pub fn read_only_files(&self, files: impl Iterator<Item = PathBuf>) -> io::Result<()> {
        for file in files {
            self.read_only(AnyCar::new(EitherMmapOrRandomAccessFile::open(file)?)?);
        }

        Ok(())
    }

    pub fn heaviest_tipset(&self) -> anyhow::Result<Tipset> {
        let tipsets = self
            .read_only
            .read()
            .iter()
            .map(AnyCar::heaviest_tipset)
            .collect::<anyhow::Result<Vec<_>>>()?;
        tipsets
            .into_iter()
            .max_by_key(Tipset::epoch)
            .context("ManyCar store doesn't have a heaviest tipset")
    }
}

impl<ReaderT: super::RandomAccessFileReader> From<AnyCar<ReaderT>> for ManyCar<MemoryDB> {
    fn from(any_car: AnyCar<ReaderT>) -> Self {
        ManyCar::default().with_read_only(any_car)
    }
}

impl TryFrom<Vec<PathBuf>> for ManyCar<MemoryDB> {
    type Error = io::Error;
    fn try_from(files: Vec<PathBuf>) -> io::Result<Self> {
        ManyCar::default().with_read_only_files(files.into_iter())
    }
}

impl<WriterT: Blockstore> Blockstore for ManyCar<WriterT> {
    fn get(&self, k: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        // Theoretically it should be easily parallelizable with `rayon`.
        // In practice, there is a massive performance loss when providing
        // more than a single reader.
        for reader in self.read_only.read().iter() {
            if let Some(val) = reader.get(k)? {
                return Ok(Some(val));
            }
        }
        self.writer.get(k)
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

    #[test]
    fn many_car_idempotent() {
        let many = ManyCar::new(MemoryDB::default())
            .with_read_only(AnyCar::try_from(mainnet::DEFAULT_GENESIS).unwrap())
            .with_read_only(AnyCar::try_from(mainnet::DEFAULT_GENESIS).unwrap());
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
        let many = ManyCar::from(AnyCar::try_from(calibnet::DEFAULT_GENESIS).unwrap());
        let heaviest = many.heaviest_tipset().unwrap();
        assert_eq!(
            heaviest.min_ticket_block(),
            &heaviest.genesis(&many).unwrap()
        );
    }
}
