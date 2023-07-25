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
    pub fn new(mk_reader: impl Fn() -> ReaderT + Clone + 'static) -> Result<Self> {
        if let Ok(forest_car) = super::ForestCar::new(mk_reader.clone()) {
            return Ok(AnyCar::Forest(forest_car));
        }
        if let Ok(plain_car) = super::PlainCar::new(mk_reader()) {
            return Ok(AnyCar::Plain(plain_car));
        }
        if let Ok(decompressed) = zstd::stream::decode_all(mk_reader()) {
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
