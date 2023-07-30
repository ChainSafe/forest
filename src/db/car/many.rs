use super::AnyCar;
use super::ZstdFrameCache;
use crate::blocks::Tipset;
use crate::db::MemoryDB;
use anyhow::Context;
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use lru::LruCache;
use nonzero_ext::nonzero;
use parking_lot::Mutex;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;

pub struct ManyCar<WriterT> {
    shared_cache: ZstdFrameCache,
    read_only: Vec<AnyCar<Box<dyn super::CarReader>>>,
    writer: WriterT,
}

impl<WriterT: Blockstore> ManyCar<WriterT> {
    pub fn new(writer: WriterT) -> Self {
        ManyCar {
            shared_cache: Arc::new(Mutex::new(LruCache::new(nonzero!(1024_usize)))),
            read_only: Vec::new(),
            writer,
        }
    }

    pub fn read_only<ReaderT: super::CarReader>(&mut self, any_car: AnyCar<ReaderT>) {
        let key = self.read_only.len() as u64;
        self.read_only
            .push(any_car.with_cache(self.shared_cache.clone(), key).to_dyn());
    }

    pub fn read_only_files<'a>(&mut self, files: impl Iterator<Item = PathBuf>) -> io::Result<()> {
        for file in files {
            let car = AnyCar::new(move || std::fs::File::open(&file))?;
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

impl<ReaderT: super::CarReader> From<AnyCar<ReaderT>> for ManyCar<MemoryDB> {
    fn from(any_car: AnyCar<ReaderT>) -> Self {
        let mut many_car = ManyCar::new(MemoryDB::default());
        many_car.read_only(any_car);
        many_car
    }
}

impl<WriterT: Blockstore> Blockstore for ManyCar<WriterT> {
    fn get(&self, k: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
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
