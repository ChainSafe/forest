// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::num::NonZeroUsize;

use cid::Cid;
use nonzero_ext::nonzero;

use crate::utils::{cache::SizeTrackingLruCache, get_size};

/// Thread-safe cache for tracking bad blocks.
/// This cache is checked before validating a block, to ensure no duplicate
/// work.
#[derive(Debug)]
pub struct BadBlockCache {
    cache: SizeTrackingLruCache<get_size::CidWrapper, ()>,
}

impl Default for BadBlockCache {
    fn default() -> Self {
        Self::new(nonzero!(1usize << 15))
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
