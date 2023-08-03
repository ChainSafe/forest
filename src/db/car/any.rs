// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! There are three different CAR formats: `.car`, `.car.zst` and
//! `.forest.car.zst`. [`AnyCar`] identifies the format by inspecting the CAR
//! header and the first key-value block, and picks the appropriate block store
//! (either [`super::ForestCar`] or [`super::PlainCar`]).
//!
//! CARv2 is not supported yet.

use super::{CacheKey, ZstdFrameCache};
use crate::blocks::Tipset;
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use parking_lot::Mutex;
use std::io::{Cursor, Error, ErrorKind, Read, Result, Seek};
use std::path::PathBuf;
use std::sync::Arc;

pub enum AnyCar<ReaderT> {
    Plain(super::PlainCar<ReaderT>),
    Forest(super::ForestCar<ReaderT>),
    Memory(super::PlainCar<Cursor<Vec<u8>>>),
}

impl<ReaderT: super::CarReader> AnyCar<ReaderT> {
    /// Open an archive. May be formatted as `.car`, `.car.zst` or
    /// `.forest.car.zst`. This call may block for an indeterminate amount of
    /// time while data is decoded and indexed.
    pub fn new(mk_reader: impl super::forest::ReaderGen<ReaderT>) -> Result<Self> {
        let plain_reader = mk_reader();
        let zstd_reader = mk_reader();
        if let Ok(forest_car) = super::ForestCar::new(mk_reader) {
            return Ok(AnyCar::Forest(forest_car));
        }
        if let Ok(plain_car) = super::PlainCar::new(plain_reader?) {
            return Ok(AnyCar::Plain(plain_car));
        }
        // Maybe use a tempfile for this in the future.
        if let Ok(decompressed) = zstd::stream::decode_all(zstd_reader?) {
            let mem_reader = Cursor::new(decompressed);
            if let Ok(mem_car) = super::PlainCar::new(mem_reader) {
                return Ok(AnyCar::Memory(mem_car));
            }
        }
        Err(Error::new(
            ErrorKind::InvalidData,
            "input not recognized as any kind of CAR data (.car, .car.zst, .forest.car)",
        ))
    }

    /// Filecoin archives are tagged with the heaviest tipset. This call may
    /// fail if the archive is corrupt or if it is not a Filecoin archive.
    pub fn heaviest_tipset(&self) -> anyhow::Result<Tipset> {
        match self {
            AnyCar::Forest(forest) => forest.heaviest_tipset(),
            AnyCar::Plain(plain) => plain.heaviest_tipset(),
            AnyCar::Memory(mem) => mem.heaviest_tipset(),
        }
    }

    /// Return the identified CAR format variant. There are three variants:
    /// `CARv1`, `CARv1.zst` and `ForestCARv1.zst`.
    pub fn variant(&self) -> &'static str {
        match self {
            AnyCar::Forest(_) => "ForestCARv1.zst",
            AnyCar::Plain(_) => "CARv1",
            AnyCar::Memory(_) => "CARv1.zst",
        }
    }

    /// Discard reader type and replace with dynamic trait object.
    pub fn into_dyn(self) -> AnyCar<Box<dyn super::CarReader>> {
        match self {
            AnyCar::Forest(f) => AnyCar::Forest(f.into_dyn()),
            AnyCar::Plain(p) => AnyCar::Plain(p.into_dyn()),
            AnyCar::Memory(m) => AnyCar::Memory(m),
        }
    }

    /// Set the z-frame cache of the inner CAR reader.
    pub fn with_cache(self, cache: Arc<Mutex<ZstdFrameCache>>, key: CacheKey) -> Self {
        match self {
            AnyCar::Forest(f) => AnyCar::Forest(f.with_cache(cache, key)),
            AnyCar::Plain(p) => AnyCar::Plain(p),
            AnyCar::Memory(m) => AnyCar::Memory(m),
        }
    }
}

impl TryFrom<&'static [u8]> for AnyCar<std::io::Cursor<&'static [u8]>> {
    type Error = std::io::Error;
    fn try_from(bytes: &'static [u8]) -> std::io::Result<Self> {
        Ok(AnyCar::Plain(super::PlainCar::new(std::io::Cursor::new(
            bytes,
        ))?))
    }
}

impl TryFrom<PathBuf> for AnyCar<std::fs::File> {
    type Error = std::io::Error;
    fn try_from(path: PathBuf) -> std::io::Result<Self> {
        AnyCar::new(move || std::fs::File::open(&path))
    }
}

impl<ReaderT> Blockstore for AnyCar<ReaderT>
where
    ReaderT: Read + Seek,
{
    fn get(&self, k: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        match self {
            AnyCar::Forest(forest) => forest.get(k),
            AnyCar::Plain(plain) => plain.get(k),
            AnyCar::Memory(mem) => mem.get(k),
        }
    }

    fn put_keyed(&self, k: &Cid, block: &[u8]) -> anyhow::Result<()> {
        match self {
            AnyCar::Forest(forest) => forest.put_keyed(k, block),
            AnyCar::Plain(plain) => plain.put_keyed(k, block),
            AnyCar::Memory(mem) => mem.put_keyed(k, block),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::networks::{calibnet, mainnet};
    use std::io::Cursor;

    #[test]
    fn forest_any_load_calibnet() {
        let forest_car = AnyCar::new(move || Ok(Cursor::new(calibnet::DEFAULT_GENESIS))).unwrap();
        assert!(forest_car.has(&calibnet::GENESIS_CID).unwrap());
    }

    #[test]
    fn forest_any_load_calibnet_zstd() {
        let forest_car =
            AnyCar::new(move || Ok(Cursor::new(zstd::encode_all(calibnet::DEFAULT_GENESIS, 3)?)))
                .unwrap();
        assert!(forest_car.has(&calibnet::GENESIS_CID).unwrap());
    }

    #[test]
    fn forest_any_load_mainnet() {
        let forest_car = AnyCar::new(move || Ok(Cursor::new(mainnet::DEFAULT_GENESIS))).unwrap();
        assert!(forest_car.has(&mainnet::GENESIS_CID).unwrap());
    }

    #[test]
    fn forest_any_load_mainnet_zstd() {
        let forest_car =
            AnyCar::new(move || Ok(Cursor::new(zstd::encode_all(mainnet::DEFAULT_GENESIS, 3)?)))
                .unwrap();
        assert!(forest_car.has(&mainnet::GENESIS_CID).unwrap());
    }
}
