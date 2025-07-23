// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
mod any;
pub mod forest;
mod many;
pub mod plain;

pub use any::AnyCar;
pub use forest::ForestCar;
use get_size2::GetSize as _;
pub use many::ManyCar;
pub use plain::PlainCar;

use ahash::HashMap;
use cid::Cid;
use lru::LruCache;
use positioned_io::{ReadAt, Size};
use std::{num::NonZeroUsize, sync::LazyLock};

use crate::utils::get_size::CidWrapper;

pub trait RandomAccessFileReader: ReadAt + Size + Send + Sync + 'static {}
impl<X: ReadAt + Size + Send + Sync + 'static> RandomAccessFileReader for X {}

/// Multiple `.forest.car.zst` archives may use the same cache, each with a
/// unique cache key.
pub type CacheKey = u64;

type FrameOffset = u64;

// 1 GiB
pub static ZSTD_FRAME_CACHE_DEFAULT_MAX_SIZE: LazyLock<usize> = LazyLock::new(|| {
    const ENV_KEY: &str = "FOREST_ZSTD_FRAME_CACHE_DEFAULT_MAX_SIZE";
    if let Ok(value) = std::env::var(ENV_KEY) {
        if let Ok(size) = value.parse::<NonZeroUsize>() {
            let size = size.get();
            tracing::info!("zstd frame max size is set to {size} via {ENV_KEY}");
            return size;
        } else {
            tracing::warn!("Failed to parse {ENV_KEY}={value}, value should be a positive integer");
        }
    }
    // 1GiB
    1024 * 1024 * 1024
});

pub struct ZstdFrameCache {
    /// Maximum size in bytes. Pages will be evicted if the total size of the
    /// cache exceeds this amount.
    pub max_size: usize,
    current_size: usize,
    lru: LruCache<(FrameOffset, CacheKey), HashMap<CidWrapper, Vec<u8>>>,
}

impl Default for ZstdFrameCache {
    fn default() -> Self {
        ZstdFrameCache::new(*ZSTD_FRAME_CACHE_DEFAULT_MAX_SIZE)
    }
}

impl ZstdFrameCache {
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
            .map(|index| index.get(&cid.into()).cloned())
    }

    /// Insert entry into lru-cache and evict pages if `max_size` has been exceeded.
    pub fn put(&mut self, offset: FrameOffset, key: CacheKey, index: HashMap<CidWrapper, Vec<u8>>) {
        let lru_key = (offset, key);
        let lru_key_size = lru_key.get_size();
        let entry_size = index.get_size();
        if let Some((_, prev_entry)) = self.lru.push(lru_key, index) {
            // keys are cancelled out
            self.current_size = self
                .current_size
                .saturating_add(entry_size)
                .saturating_sub(prev_entry.get_size());
        } else {
            self.current_size = self
                .current_size
                .saturating_add(entry_size)
                .saturating_add(lru_key_size);
        }
        while self.current_size > self.max_size {
            if let Some((prev_key, prev_entry)) = self.lru.pop_lru() {
                self.current_size = self
                    .current_size
                    .saturating_sub(prev_key.get_size())
                    .saturating_sub(prev_entry.get_size());
            } else {
                break;
            }
        }
    }
}
