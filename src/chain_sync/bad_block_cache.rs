// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::num::NonZeroUsize;

use cid::Cid;
use nonzero_ext::nonzero;

use crate::utils::{ShallowClone, cache::SizeTrackingLruCache, get_size};

/// Default capacity for CID caches (32768 entries).
/// That's about 4 MiB.
const DEFAULT_CID_CACHE_CAPACITY: NonZeroUsize = nonzero!(1usize << 15);

/// Thread-safe cache for tracking bad blocks.
/// This cache is checked before validating a block, to ensure no duplicate
/// work.
#[derive(Debug)]
pub struct BadBlockCache {
    cache: SizeTrackingLruCache<get_size::CidWrapper, ()>,
}

impl Default for BadBlockCache {
    fn default() -> Self {
        Self::new(DEFAULT_CID_CACHE_CAPACITY)
    }
}

impl BadBlockCache {
    pub fn new(cap: NonZeroUsize) -> Self {
        Self {
            cache: SizeTrackingLruCache::new_with_metrics("bad_block".into(), cap),
        }
    }

    pub fn push(&self, c: Cid) {
        self.cache.push(c.into(), ());
        tracing::warn!("Marked bad block: {c}");
    }

    /// Returns `Some` if the block CID is in bad block cache.
    /// This function does not update the head position of the `Cid` key.
    pub fn peek(&self, c: &Cid) -> Option<()> {
        self.cache.peek_cloned(&(*c).into())
    }
}

/// Thread-safe LRU cache for tracking recently seen gossip block CIDs.
/// Used to de-duplicate gossip blocks before expensive message fetching.
#[derive(Debug)]
pub struct SeenBlockCache {
    cache: SizeTrackingLruCache<get_size::CidWrapper, ()>,
}

impl ShallowClone for SeenBlockCache {
    fn shallow_clone(&self) -> Self {
        Self {
            cache: self.cache.shallow_clone(),
        }
    }
}

impl Default for SeenBlockCache {
    fn default() -> Self {
        Self::new(DEFAULT_CID_CACHE_CAPACITY)
    }
}

impl SeenBlockCache {
    pub fn new(cap: NonZeroUsize) -> Self {
        Self {
            cache: SizeTrackingLruCache::new_with_metrics("seen_gossip_block".into(), cap),
        }
    }

    /// Returns `true` if the CID was already present (duplicate).
    /// Always inserts/refreshes the entry.
    pub fn test_and_insert(&self, c: &Cid) -> bool {
        self.cache.push((*c).into(), ()).is_some()
    }
}
