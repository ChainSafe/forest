// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use std::io::{Cursor, Error, ErrorKind, Read, Result, Seek};

pub enum AnyCar<ReaderT> {
    Plain(super::PlainCar<ReaderT>),
    Forest(super::ForestCar<ReaderT>),
    Memory(super::PlainCar<Cursor<Vec<u8>>>),
}

impl<ReaderT: Read + Seek> AnyCar<ReaderT> {
    pub fn new(mk_reader: impl Fn() -> Result<ReaderT> + Clone + 'static) -> Result<Self> {
        if let Ok(forest_car) = super::ForestCar::new(mk_reader.clone()) {
            return Ok(AnyCar::Forest(forest_car));
        }
        if let Ok(plain_car) = super::PlainCar::new(mk_reader()?) {
            return Ok(AnyCar::Plain(plain_car));
        }
        // Maybe use a tempfile for this in the future.
        if let Ok(decompressed) = zstd::stream::decode_all(mk_reader()?) {
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

    pub fn roots(&self) -> Vec<Cid> {
        match self {
            AnyCar::Forest(forest) => forest.roots(),
            AnyCar::Plain(plain) => plain.roots(),
            AnyCar::Memory(mem) => mem.roots(),
        }
    }

    pub fn variant(&self) -> &'static str {
        match self {
            AnyCar::Forest(_) => "ForestCARv1.zst",
            AnyCar::Plain(_) => "CARv1",
            AnyCar::Memory(_) => "CARv1.zst",
        }
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
