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

use cid::Cid;
use positioned_io::{ReadAt, Size};
use std::{
    num::NonZeroUsize,
    sync::{
        LazyLock,
        atomic::{AtomicUsize, Ordering},
    },
};

use crate::utils::{cache::SizeTrackingLruCache, get_size::CidWrapper};

pub trait RandomAccessFileReader: ReadAt + Size + Send + Sync + 'static {}
impl<X: ReadAt + Size + Send + Sync + 'static> RandomAccessFileReader for X {}

/// Multiple `.forest.car.zst` archives may use the same cache, each with a
/// unique cache key.
pub type CacheKey = u64;

type FrameOffset = u64;

/// According to FRC-0108, `v2` snapshots have exactly one root pointing to metadata
const V2_SNAPSHOT_ROOT_COUNT: usize = 1;

pub static ZSTD_FRAME_CACHE_DEFAULT_MAX_SIZE: LazyLock<usize> = LazyLock::new(|| {
    const ENV_KEY: &str = "FOREST_ZSTD_FRAME_CACHE_DEFAULT_MAX_SIZE";
    if let Ok(value) = std::env::var(ENV_KEY) {
        if let Ok(size) = value.parse::<NonZeroUsize>() {
            let size = size.get();
            tracing::info!("zstd frame max size is set to {size} via {ENV_KEY}");
            return size;
        } else {
            tracing::error!(
                "Failed to parse {ENV_KEY}={value}, value should be a positive integer"
            );
        }
    }
    // 256 MiB
    256 * 1024 * 1024
});

pub struct ZstdFrameCache {
    /// Maximum size in bytes. Pages will be evicted if the total size of the
    /// cache exceeds this amount.
    pub max_size: usize,
    current_size: AtomicUsize,
    // use `hashbrown::HashMap` here because its `GetSize` implementation is accurate
    // (thanks to `hashbrown::HashMap::allocation_size`).
    lru: SizeTrackingLruCache<(FrameOffset, CacheKey), hashbrown::HashMap<CidWrapper, Vec<u8>>>,
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
            current_size: AtomicUsize::new(0),
            lru: SizeTrackingLruCache::unbounded_with_metrics("zstd_frame".into()),
        }
    }

    /// Return a clone of the value associated with `cid`. If a value is found,
    /// the cache entry is moved to the top of the queue.
    pub fn get(&self, offset: FrameOffset, key: CacheKey, cid: Cid) -> Option<Option<Vec<u8>>> {
        self.lru
            .cache()
            .write()
            .get(&(offset, key))
            .map(|index| index.get(&CidWrapper::from(cid)).cloned())
    }

    /// Insert entry into lru-cache and evict pages if `max_size` has been exceeded.
    pub fn put(
        &self,
        offset: FrameOffset,
        key: CacheKey,
        mut index: hashbrown::HashMap<CidWrapper, Vec<u8>>,
    ) {
        index.shrink_to_fit();

        let lru_key = (offset, key);
        let lru_key_size = lru_key.get_size();
        let entry_size = index.get_size();
        // Skip large items
        if entry_size.saturating_add(lru_key_size) >= self.max_size {
            return;
        }

        if let Some(prev_entry) = self.lru.push(lru_key, index) {
            // keys are cancelled out
            self.current_size.fetch_add(entry_size, Ordering::Relaxed);
            self.current_size
                .fetch_sub(prev_entry.get_size(), Ordering::Relaxed);
        } else {
            self.current_size
                .fetch_add(entry_size.saturating_add(lru_key_size), Ordering::Relaxed);
        }
        while self.current_size.load(Ordering::Relaxed) > self.max_size {
            if let Some((prev_key, prev_entry)) = self.lru.pop_lru() {
                self.current_size.fetch_sub(
                    prev_key.get_size().saturating_add(prev_entry.get_size()),
                    Ordering::Relaxed,
                );
            } else {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::{multihash::MultihashCode, rand::forest_rng};
    use fvm_ipld_encoding::IPLD_RAW;
    use multihash_derive::MultihashDigest;
    use rand::Rng;

    #[test]
    fn test_zstd_frame_cache_size() {
        let mut rng = forest_rng();
        let cache = ZstdFrameCache::new(10);
        for i in 0..100 {
            let index = gen_index(&mut rng);
            cache.put(i, i, index);
            assert_eq!(
                cache.current_size.load(Ordering::Relaxed),
                cache.lru.size_in_bytes()
            );
            let index2 = gen_index(&mut rng);
            cache.put(i, i, index2);
            assert_eq!(
                cache.current_size.load(Ordering::Relaxed),
                cache.lru.size_in_bytes()
            );
        }
    }

    fn gen_index(rng: &mut impl Rng) -> hashbrown::HashMap<CidWrapper, Vec<u8>> {
        let mut map = hashbrown::HashMap::default();
        for _ in 0..10 {
            let vec_len = rng.gen_range(64..1024);
            let mut data = vec![0; vec_len];
            rng.fill_bytes(&mut data);
            let cid = Cid::new_v1(IPLD_RAW, MultihashCode::Blake2b256.digest(&data));
            map.insert(cid.into(), data);
        }
        map
    }
}
