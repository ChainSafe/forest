// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::num::NonZeroUsize;

use nonzero_ext::nonzero;

use crate::prelude::*;
use crate::utils::cache::SizeTrackingCache;

/// Default capacity for CID caches (32768 entries).
/// That's about 4 MiB.
const DEFAULT_CID_CACHE_CAPACITY: NonZeroUsize = nonzero!(1usize << 15);

/// Thread-safe cache for tracking bad blocks.
/// This cache is checked before validating a block, to ensure no duplicate
/// work.
#[derive(Debug)]
pub struct BadBlockCache {
    cache: SizeTrackingCache<CidWrapper, ()>,
}

impl Default for BadBlockCache {
    fn default() -> Self {
        Self::new(DEFAULT_CID_CACHE_CAPACITY)
    }
}

impl ShallowClone for BadBlockCache {
    fn shallow_clone(&self) -> Self {
        Self {
            cache: self.cache.shallow_clone(),
        }
    }
}

impl BadBlockCache {
    pub fn new(cap: NonZeroUsize) -> Self {
        Self {
            cache: SizeTrackingCache::new_with_metrics("bad_block", cap),
        }
    }

    pub fn push(&self, c: Cid) {
        self.cache.push(c.into(), ());
        tracing::warn!("Marked bad block: {c}");
    }

    pub fn get(&self, c: &Cid) -> Option<()> {
        self.cache.get_cloned(c)
    }

    pub fn clear(&self) {
        self.cache.clear()
    }
}

/// Thread-safe cache for tracking recently seen gossip block CIDs.
/// Used to de-duplicate gossip blocks before expensive message fetching.
#[derive(Debug)]
pub struct SeenBlockCache {
    cache: SizeTrackingCache<CidWrapper, ()>,
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
            cache: SizeTrackingCache::new_with_metrics("seen_gossip_block", cap),
        }
    }

    /// Returns `true` if the CID was already present (duplicate).
    /// Always inserts/refreshes the entry.
    pub fn test_and_insert(&self, c: &Cid) -> bool {
        self.cache.push_and_get_prev((*c).into(), ()).is_some()
    }
}
