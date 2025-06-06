// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use lru::LruCache;
use parking_lot::Mutex;
use std::{
    num::NonZeroUsize,
    sync::{
        Arc,
        atomic::{self, AtomicUsize},
    },
};

pub trait BlockstoreReadCache {
    fn get(&self, k: &Cid) -> Option<Vec<u8>>;

    fn put(&self, k: Cid, block: Vec<u8>);

    fn len(&self) -> usize;

    fn size_in_bytes(&self) -> usize;
}

pub struct LruBlockstoreReadCache {
    lru: Mutex<LruCache<Cid, Vec<u8>>>,
    size_in_bytes: AtomicUsize,
}

impl LruBlockstoreReadCache {
    pub fn new(cap: NonZeroUsize) -> Self {
        Self {
            lru: Mutex::new(LruCache::new(cap)),
            size_in_bytes: AtomicUsize::default(),
        }
    }
}

impl BlockstoreReadCache for LruBlockstoreReadCache {
    fn get(&self, k: &Cid) -> Option<Vec<u8>> {
        self.lru.lock().get(k).cloned()
    }

    fn put(&self, k: Cid, block: Vec<u8>) {
        let block_size = block.len();
        if let Some((_, old_block)) = self.lru.lock().push(k, block) {
            let old_block_size = old_block.len();
            if block_size >= old_block_size {
                self.size_in_bytes
                    .fetch_add(block_size - old_block_size, atomic::Ordering::Relaxed);
            } else {
                self.size_in_bytes
                    .fetch_sub(old_block_size - block_size, atomic::Ordering::Relaxed);
            }
        } else {
            self.size_in_bytes.fetch_add(
                std::mem::size_of::<Cid>() + block_size,
                atomic::Ordering::Relaxed,
            );
        }
    }

    fn len(&self) -> usize {
        self.lru.lock().len()
    }

    fn size_in_bytes(&self) -> usize {
        self.size_in_bytes.load(atomic::Ordering::Relaxed)
    }
}

#[derive(Debug, Default)]
pub struct VoidBlockstoreReadCache;

impl BlockstoreReadCache for VoidBlockstoreReadCache {
    fn get(&self, _: &Cid) -> Option<Vec<u8>> {
        None
    }

    fn put(&self, _: Cid, _: Vec<u8>) {}

    fn len(&self) -> usize {
        0
    }

    fn size_in_bytes(&self) -> usize {
        0
    }
}

impl<T: BlockstoreReadCache> BlockstoreReadCache for Arc<T> {
    fn get(&self, k: &Cid) -> Option<Vec<u8>> {
        self.as_ref().get(k)
    }

    fn put(&self, k: Cid, block: Vec<u8>) {
        self.as_ref().put(k, block)
    }

    fn len(&self) -> usize {
        self.as_ref().len()
    }

    fn size_in_bytes(&self) -> usize {
        self.as_ref().size_in_bytes()
    }
}

pub trait BlockstoreReadCacheStats {
    fn hit(&self) -> usize;

    fn track_hit(&self);

    fn miss(&self) -> usize;

    fn track_miss(&self);
}

#[derive(Debug, Default)]
pub struct DefaultBlockstoreReadCacheStats {
    hit: AtomicUsize,
    miss: AtomicUsize,
}

impl BlockstoreReadCacheStats for DefaultBlockstoreReadCacheStats {
    fn hit(&self) -> usize {
        self.hit.load(atomic::Ordering::Relaxed)
    }

    fn track_hit(&self) {
        self.hit.fetch_add(1, atomic::Ordering::Relaxed);
    }

    fn miss(&self) -> usize {
        self.miss.load(atomic::Ordering::Relaxed)
    }

    fn track_miss(&self) {
        self.miss.fetch_add(1, atomic::Ordering::Relaxed);
    }
}

pub struct BlockstoreWithReadCache<
    DB: Blockstore,
    CACHE: BlockstoreReadCache,
    STATS: BlockstoreReadCacheStats,
> {
    inner: DB,
    cache: CACHE,
    stats: Option<STATS>,
}

impl<DB: Blockstore, CACHE: BlockstoreReadCache, STATS: BlockstoreReadCacheStats>
    BlockstoreWithReadCache<DB, CACHE, STATS>
{
    pub fn new(db: DB, cache: CACHE, stats: Option<STATS>) -> Self {
        Self {
            inner: db,
            cache,
            stats,
        }
    }

    pub fn stats(&self) -> Option<&STATS> {
        self.stats.as_ref()
    }
}

impl<DB: Blockstore, CACHE: BlockstoreReadCache, STATS: BlockstoreReadCacheStats> Blockstore
    for BlockstoreWithReadCache<DB, CACHE, STATS>
{
    fn get(&self, k: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        if let Some(cached) = self.cache.get(k) {
            self.stats.as_ref().map(BlockstoreReadCacheStats::track_hit);
            Ok(Some(cached))
        } else {
            let block = self.inner.get(k)?;
            self.stats
                .as_ref()
                .map(BlockstoreReadCacheStats::track_miss);
            if let Some(block) = &block {
                self.cache.put(*k, block.clone());
            }
            Ok(block)
        }
    }

    fn put_keyed(&self, k: &Cid, block: &[u8]) -> anyhow::Result<()> {
        self.inner.put_keyed(k, block)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db::MemoryDB, utils::rand::forest_rng};
    use fvm_ipld_encoding::DAG_CBOR;
    use multihash_codetable::Code::Blake2b256;
    use multihash_codetable::MultihashDigest as _;
    use rand::Rng as _;

    #[test]
    fn test_blockstore_read_cache() {
        const N_RECORDS: usize = 4;
        const CACHE_SIZE: usize = 2;
        let mem_db = Arc::new(MemoryDB::default());
        let mut records = Vec::with_capacity(N_RECORDS);
        for _ in 0..N_RECORDS {
            let mut record = [0; 1024];
            forest_rng().fill(&mut record);
            let key = Cid::new_v1(DAG_CBOR, Blake2b256.digest(record.as_slice()));
            mem_db.put_keyed(&key, &record).unwrap();
            records.push((key, record));
        }
        let cache = Arc::new(LruBlockstoreReadCache::new(CACHE_SIZE.try_into().unwrap()));
        let db = BlockstoreWithReadCache::new(
            mem_db.clone(),
            cache.clone(),
            Some(DefaultBlockstoreReadCacheStats::default()),
        );

        assert_eq!(cache.len(), 0);
        assert_eq!(db.stats().unwrap().hit(), 0);
        assert_eq!(db.stats().unwrap().miss(), 0);

        for (i, (k, v)) in records.iter().enumerate() {
            assert_eq!(&db.get(k).unwrap().unwrap(), v);

            assert_eq!(cache.len(), CACHE_SIZE.min(i + 1));
            assert_eq!(db.stats().unwrap().hit(), i);
            assert_eq!(db.stats().unwrap().miss(), i + 1);

            assert_eq!(&db.get(k).unwrap().unwrap(), v);

            assert_eq!(cache.len(), CACHE_SIZE.min(i + 1));
            assert_eq!(db.stats().unwrap().hit(), i + 1);
            assert_eq!(db.stats().unwrap().miss(), i + 1);
        }

        let (k0, v0) = &records[0];

        assert_eq!(&db.get(k0).unwrap().unwrap(), v0);

        assert_eq!(cache.len(), CACHE_SIZE);
        assert_eq!(db.stats().unwrap().hit(), 4);
        assert_eq!(db.stats().unwrap().miss(), 5);

        assert_eq!(&db.get(k0).unwrap().unwrap(), v0);

        assert_eq!(cache.len(), CACHE_SIZE);
        assert_eq!(db.stats().unwrap().hit(), 5);
        assert_eq!(db.stats().unwrap().miss(), 5);
    }
}
