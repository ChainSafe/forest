// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
mod any;
pub mod forest;
mod many;
pub mod plain;

pub use any::AnyCar;
pub use forest::ForestCar;
pub use many::ManyCar;
pub use plain::PlainCar;

use crate::utils::db::car_index::FrameOffset;
use ahash::HashMap;
use cid::Cid;
use lru::LruCache;
use positioned_io::{ReadAt, Size};

pub trait RandomAccessFileReader: ReadAt + Size + Send + Sync + 'static {}
impl<X: ReadAt + Size + Send + Sync + 'static> RandomAccessFileReader for X {}

// Something to be contributed upstream.
// Similar to https://doc.rust-lang.org/1.38.0/src/std/io/impls.rs.html#122-143.
impl ReadAt for Box<dyn RandomAccessFileReader> {
    #[inline]
    fn read_at(&self, pos: u64, buf: &mut [u8]) -> std::io::Result<usize> {
        (**self).read_at(pos, buf)
    }

    #[inline]
    fn read_exact_at(&self, pos: u64, buf: &mut [u8]) -> std::io::Result<()> {
        (**self).read_exact_at(pos, buf)
    }
}

impl Size for Box<dyn RandomAccessFileReader> {
    #[inline]
    fn size(&self) -> std::io::Result<Option<u64>> {
        (**self).size()
    }
}

/// Multiple `.forest.car.zst` archives may use the same cache, each with a
/// unique cache key.
pub type CacheKey = u64;

pub struct ZstdFrameCache {
    /// Maximum size in bytes. Pages will be evicted if the total size of the
    /// cache exceeds this amount.
    pub max_size: usize,
    current_size: usize,
    lru: LruCache<(FrameOffset, CacheKey), HashMap<Cid, Vec<u8>>>,
}

impl Default for ZstdFrameCache {
    fn default() -> Self {
        ZstdFrameCache::new(ZstdFrameCache::DEFAULT_SIZE)
    }
}

impl ZstdFrameCache {
    // 1 GiB
    pub const DEFAULT_SIZE: usize = 1024 * 1024 * 1024;

    pub fn new(max_size: usize) -> Self {
        ZstdFrameCache {
            max_size,
            current_size: 0,
            lru: LruCache::unbounded(),
        }
    }

    /// Return a clone of the value associated with `cid`. If a value is found,
    /// the cache entry is moved to the top of the queue.
    pub fn get(&mut self, offset: FrameOffset, key: CacheKey, cid: Cid) -> Option<Option<Vec<u8>>> {
        self.lru
            .get(&(offset, key))
            .map(|index| index.get(&cid).cloned())
    }

    /// Insert entry into lru-cache and evict pages if `max_size` has been exceeded.
    pub fn put(&mut self, offset: FrameOffset, key: CacheKey, index: HashMap<Cid, Vec<u8>>) {
        fn size_of_entry(entry: &HashMap<Cid, Vec<u8>>) -> usize {
            entry.values().map(Vec::len).sum::<usize>()
        }
        self.current_size += size_of_entry(&index);
        if let Some(prev_entry) = self.lru.put((offset, key), index) {
            self.current_size -= size_of_entry(&prev_entry);
        }
        while self.current_size > self.max_size {
            if let Some((_, entry)) = self.lru.pop_lru() {
                self.current_size -= size_of_entry(&entry)
            } else {
                break;
            }
        }
    }
}
