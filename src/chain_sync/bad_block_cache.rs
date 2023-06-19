// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::num::NonZeroUsize;

use cid::Cid;
use lru::LruCache;
use nonzero_ext::nonzero;
use parking_lot::Mutex;

/// Thread-safe cache for tracking bad blocks.
/// This cache is checked before validating a block, to ensure no duplicate
/// work.
#[derive(Debug)]
pub struct BadBlockCache {
    cache: Mutex<LruCache<Cid, String>>,
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
        self.cache.lock().put(c, reason)
    }

    /// Returns `Some` with the reason if the block CID is in bad block cache.
    /// This also updates the key to the head of the cache.
    pub fn get(&self, c: &Cid) -> Option<String> {
        self.cache.lock().get(c).cloned()
    }

    /// Returns `Some` with the reason if the block CID is in bad block cache.
    /// This function does not update the head position of the `Cid` key.
    pub fn peek(&self, c: &Cid) -> Option<String> {
        self.cache.lock().peek(c).cloned()
    }
}
