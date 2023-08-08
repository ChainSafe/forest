// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! The [`ManyCar`] block store is the union of `N` read-only CAR-backed block
//! stores and a single writable block store. Get requests are forwarded to each
//! store (including the writable store) and the first hit is returned. Write
//! requests are only forwarded to the writable store.
//!
//! A single z-frame cache is shared between all read-only stores.

use super::{AnyCar, ZstdFrameCache};
use crate::blocks::Tipset;
use crate::db::MemoryDB;
use crate::utils::io::random_access::RandomAccessFile;
use anyhow::Context;
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use parking_lot::Mutex;
use std::{io, path::PathBuf, sync::Arc};

pub struct ManyCar<WriterT = MemoryDB> {
    shared_cache: Arc<Mutex<ZstdFrameCache>>,
    read_only: Vec<AnyCar<Box<dyn super::RandomAccessFileReader>>>,
    writer: WriterT,
}

impl ManyCar {
    pub fn new() -> Self {
        ManyCar {
            shared_cache: Arc::new(Mutex::new(ZstdFrameCache::default())),
            read_only: Vec::new(),
            writer: MemoryDB::default(),
        }
    }
}

impl<WriterT> ManyCar<WriterT> {
    pub fn read_only<ReaderT: super::RandomAccessFileReader>(&mut self, any_car: AnyCar<ReaderT>) {
        let key = self.read_only.len() as u64;
        self.read_only.push(
            any_car
                .with_cache(self.shared_cache.clone(), key)
                .into_dyn(),
        );
    }

    pub fn read_only_files(&mut self, files: impl Iterator<Item = PathBuf>) -> io::Result<()> {
        for file in files {
            let car = AnyCar::new(RandomAccessFile::open(file)?)?;
            self.read_only(car);
        }
        Ok(())
    }

    pub fn heaviest_tipset(&self) -> anyhow::Result<Tipset> {
        let tipsets = self
            .read_only
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
        let mut many_car = ManyCar::new();
        many_car.read_only(any_car);
        many_car
    }
}

impl TryFrom<Vec<PathBuf>> for ManyCar<MemoryDB> {
    type Error = io::Error;
    fn try_from(files: Vec<PathBuf>) -> io::Result<Self> {
        let mut many_car = ManyCar::new();
        many_car.read_only_files(files.into_iter())?;
        Ok(many_car)
    }
}

impl<WriterT: Blockstore> Blockstore for ManyCar<WriterT> {
    fn get(&self, k: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        // Theoretically it should be easily parallelizable with `rayon`.
        // In practice, there is a massive performance loss when providing
        // more than a single reader.
        for reader in self.read_only.iter() {
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

#[cfg(test)]
mod tests {
    use super::super::AnyCar;
    use super::*;
    use crate::networks::{calibnet, mainnet};

    #[test]
    fn many_car_empty() {
        let many = ManyCar::new();
        assert!(many.heaviest_tipset().is_err());
    }

    #[test]
    fn many_car_idempotent() {
        let mut many = ManyCar::new();
        many.read_only(AnyCar::try_from(mainnet::DEFAULT_GENESIS).unwrap());
        many.read_only(AnyCar::try_from(mainnet::DEFAULT_GENESIS).unwrap());
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
