use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use std::io::{Cursor, Error, ErrorKind, Read, Result, Seek};

pub enum AnyCar<ReaderT> {
    PlainCar(super::PlainCar<ReaderT>),
    ForestCar(super::ForestCar<ReaderT>),
    MemoryCar(super::PlainCar<Cursor<Vec<u8>>>),
}

impl<ReaderT: Read + Seek> AnyCar<ReaderT> {
    pub fn new(mk_reader: impl Fn() -> ReaderT + Clone + 'static) -> Result<Self> {
        if let Ok(forest_car) = super::ForestCar::new(mk_reader.clone()) {
            return Ok(AnyCar::ForestCar(forest_car));
        }
        if let Ok(plain_car) = super::PlainCar::new(mk_reader()) {
            return Ok(AnyCar::PlainCar(plain_car));
        }
        if let Ok(decompressed) = zstd::stream::decode_all(mk_reader()) {
            let mem_reader = Cursor::new(decompressed);
            if let Ok(mem_car) = super::PlainCar::new(mem_reader) {
                return Ok(AnyCar::MemoryCar(mem_car));
            }
        }
        Err(Error::new(
            ErrorKind::InvalidData,
            "input not recognized as any kind of CAR data (.car, .car.zst, .forest.car)",
        ))
    }

    pub fn roots(&self) -> Vec<Cid> {
        match self {
            AnyCar::ForestCar(forest) => forest.roots(),
            AnyCar::PlainCar(plain) => plain.roots(),
            AnyCar::MemoryCar(mem) => mem.roots(),
        }
    }
}

impl<ReaderT> Blockstore for AnyCar<ReaderT>
where
    ReaderT: Read + Seek,
{
    fn get(&self, k: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        match self {
            AnyCar::ForestCar(forest) => forest.get(k),
            AnyCar::PlainCar(plain) => plain.get(k),
            AnyCar::MemoryCar(mem) => mem.get(k),
        }
    }

    fn put_keyed(&self, k: &Cid, block: &[u8]) -> anyhow::Result<()> {
        match self {
            AnyCar::ForestCar(forest) => forest.put_keyed(k, block),
            AnyCar::PlainCar(plain) => plain.put_keyed(k, block),
            AnyCar::MemoryCar(mem) => mem.put_keyed(k, block),
        }
    }
}
