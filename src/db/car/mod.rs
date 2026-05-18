// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
mod any;
pub mod forest;
mod many;
pub mod plain;

pub use any::AnyCar;
pub use forest::ForestCar;
use get_size2::GetSize as _;
pub use many::{ManyCar, ReloadableManyCar};
pub use plain::PlainCar;

use bytes::Bytes;
use positioned_io::{ReadAt, Size};
use std::{num::NonZeroUsize, sync::LazyLock};

use crate::prelude::*;
use crate::utils::get_size::CidWrapper;
use quick_cache::Weighter;
use quick_cache::sync::Cache as QuickCache;

pub trait RandomAccessFileReader: ReadAt + Size + Send + Sync + 'static {}
impl<X: ReadAt + Size + Send + Sync + 'static> RandomAccessFileReader for X {}

/// Multiple `.forest.car.zst` archives may use the same cache, each with a
/// unique cache key.
pub type CacheKey = u64;

type FrameOffset = u64;

/// According to FRC-0108, v2 snapshots have exactly one root pointing to metadata
pub const V2_SNAPSHOT_ROOT_COUNT: usize = 1;

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

/// One decompressed zstd frame's index, shared via [`Arc`] so cache lookups
/// don't deep-copy the inner [`hashbrown::HashMap`]. Snapshot export hits the
/// cache once per CID; a per-call `HashMap` clone destroys throughput.
type FrameIndex = Arc<hashbrown::HashMap<CidWrapper, Bytes>>;

/// A [`Weighter`] that bills each entry by `key.get_size() + value.get_size()`.
/// Used to make [`ZstdFrameCache`] evict by byte size.
#[derive(Clone, Copy, Debug, Default)]
struct ZstdFrameWeighter;

impl Weighter<(FrameOffset, CacheKey), FrameIndex> for ZstdFrameWeighter {
    fn weight(&self, key: &(FrameOffset, CacheKey), value: &FrameIndex) -> u64 {
        // quick_cache treats weight 0 as "do not evict" — clamp to 1 so the
        // cache never silently pins entries.
        (key.get_size().saturating_add(value.get_size()) as u64).max(1)
    }
}

type ZstdFrameInner = QuickCache<(FrameOffset, CacheKey), FrameIndex, ZstdFrameWeighter>;

pub struct ZstdFrameCache {
    /// Maximum size in bytes. Pages are evicted by the cache when the total
    /// weight exceeds this amount.
    pub max_size: usize,
    cache: Arc<ZstdFrameInner>,
}

impl ShallowClone for ZstdFrameCache {
    fn shallow_clone(&self) -> Self {
        Self {
            max_size: self.max_size,
            cache: self.cache.shallow_clone(),
        }
    }
}

impl Default for ZstdFrameCache {
    fn default() -> Self {
        ZstdFrameCache::new(*ZSTD_FRAME_CACHE_DEFAULT_MAX_SIZE)
    }
}

impl ZstdFrameCache {
    pub fn new(max_size: usize) -> Self {
        // Items in this cache are decompressed zstd frame indexes — large
        // hashmaps, so we don't expect many of them. The 64 estimate is a
        // hint to quick_cache for initial table sizing only; the real bound
        // is the weight capacity.
        const ESTIMATED_ITEMS: usize = 64;
        ZstdFrameCache {
            max_size,
            cache: Arc::new(QuickCache::with_weighter(
                ESTIMATED_ITEMS,
                max_size as u64,
                ZstdFrameWeighter,
            )),
        }
    }

    /// Return a clone of the value associated with `cid`. If a value is found,
    /// the cache entry is touched (moved to the top of the eviction order).
    pub fn get(&self, offset: FrameOffset, key: CacheKey, cid: Cid) -> Option<Option<Bytes>> {
        self.cache
            .get(&(offset, key))
            .map(|index| index.get(&CidWrapper::from(cid)).cloned())
    }

    /// Insert entry into the cache. Eviction happens automatically based on
    /// weight (see [`ZstdFrameWeighter`]).
    pub fn put(
        &self,
        offset: FrameOffset,
        key: CacheKey,
        mut index: hashbrown::HashMap<CidWrapper, Bytes>,
    ) {
        index.shrink_to_fit();

        let cache_key = (offset, key);
        let cache_key_size = cache_key.get_size();
        let entry_size = index.get_size();
        // Skip individual items larger than the whole cache — they'd evict
        // everything and still not fit.
        if entry_size.saturating_add(cache_key_size) >= self.max_size {
            return;
        }
        self.cache.insert(cache_key, Arc::new(index));
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
    fn test_zstd_frame_cache_stays_under_max_size() {
        let mut rng = forest_rng();
        // Pick a non-trivial cap so a few entries fit before eviction kicks in.
        let max_size: usize = 64 * 1024;
        let cache = ZstdFrameCache::new(max_size);
        for i in 0..100 {
            cache.put(i, i, gen_index(&mut rng));
            // After every insert the live weight must remain under the cap;
            // quick_cache evicts synchronously to keep it that way.
            assert!(
                cache.cache.weight() <= max_size as u64,
                "weight {} exceeds cap {}",
                cache.cache.weight(),
                max_size
            );
        }
        // Sanity: after stuffing 100 entries into a cap-bounded cache, at
        // least one eviction must have happened.
        assert!(cache.cache.len() < 100);
    }

    fn gen_index(rng: &mut impl Rng) -> hashbrown::HashMap<CidWrapper, Bytes> {
        let mut map = hashbrown::HashMap::default();
        for _ in 0..10 {
            let vec_len = rng.gen_range(64..1024);
            let mut data = vec![0; vec_len];
            rng.fill_bytes(&mut data);
            let cid = Cid::new_v1(IPLD_RAW, MultihashCode::Blake2b256.digest(&data));
            map.insert(cid.into(), data.into());
        }
        map
    }
}
