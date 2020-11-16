// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use async_std::sync::RwLock;
use blocks::Tipset;
use cid::Cid;
use lru::LruCache;
use std::sync::Arc;

/// TODO
#[derive(Clone, PartialEq, Debug)]
pub struct TipsetMetadata {
    /// Root of aggregate state after applying tipset
    pub tipset_state_root: Cid,

    /// Receipts from all message contained within this tipset
    pub tipset_receipts_root: Cid,

    /// The actual Tipset
    // TODO This should not be keeping a tipset with the metadata
    pub tipset: Arc<Tipset>,
}

/// Tracks tipsets and their states by TipsetKeys and ChainEpoch.
pub struct TipIndex {
    // metadata allows lookup of recorded Tipsets and their state roots
    // by TipsetKey and Epoch
    // TODO this should be mapping epoch to a vector of Cids of block headers
    _metadata: RwLock<LruCache<u64, TipsetMetadata>>,
}

impl Default for TipIndex {
    fn default() -> Self {
        Self {
            _metadata: RwLock::new(LruCache::new(32 << 10)),
        }
    }
}
