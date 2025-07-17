// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::num::NonZeroUsize;

use cid::Cid;
use get_size2::GetSize;
use lru::LruCache;
use nonzero_ext::nonzero;
use parking_lot::Mutex;

use crate::metrics;

/// Thread-safe cache for tracking bad blocks.
/// This cache is checked before validating a block, to ensure no duplicate
/// work.
#[derive(Debug, GetSize)]
pub struct BadBlockCache {
    #[get_size(size_fn = cache_helper)]
    cache: Mutex<LruCache<Cid, String>>,
}

fn cache_helper(cache: &Mutex<LruCache<Cid, String>>) -> usize {
    let cache = cache.lock();
    cache.iter().map(|(k, v)| k.get_size() + v.get_size()).sum()
}

impl Default for BadBlockCache {
    fn default() -> Self {
        Self::new(nonzero!(1usize << 15))
    }
}

impl BadBlockCache {
    pub fn new(cap: NonZeroUsize) -> Self {
        Self {
            cache: Mutex::new(LruCache::new(cap)),
        }
    }

    /// Puts a bad block `Cid` in the cache with a given reason.
    pub fn put(&self, c: Cid, reason: String) -> Option<String> {
        println!("Adding bad block to cache: {} with reason: {}", c, reason);
        use get_size2::GetSize;
        let v = self.cache.lock().put(c, reason);
        crate::metrics::BAD_BLOCK_CACHE_LEN
            .get_or_create(&metrics::values::BLOCK)
            .set(self.cache.lock().len() as i64);
        crate::metrics::BAD_BLOCK_CACHE_SIZE
            .get_or_create(&metrics::values::BLOCK)
            .set(self.get_size() as i64);
        v
    }

    /// Returns `Some` with the reason if the block CID is in bad block cache.
    /// This function does not update the head position of the `Cid` key.
    pub fn peek(&self, c: &Cid) -> Option<String> {
        self.cache.lock().peek(c).cloned()
    }
}
