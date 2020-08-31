// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use ahash::AHashMap;
use async_std::sync::RwLock;
use blocks::{Tipset, TipsetKeys};
use libp2p::core::PeerId;
use lru::LruCache;
use std::time::SystemTime;

type PeerSet = AHashMap<PeerId, SystemTime>;

#[derive(Debug)]
pub struct BlockReceiptTracker {
    cache: RwLock<LruCache<TipsetKeys, PeerSet>>,
}

impl Default for BlockReceiptTracker {
    fn default() -> Self {
        Self::new(512)
    }
}

impl BlockReceiptTracker {
    pub fn new(cap: usize) -> Self {
        Self {
            cache: RwLock::new(LruCache::new(cap)),
        }
    }

    pub async fn add(&self, p: PeerId, ts: Tipset) -> Option<SystemTime> {
        let current_time = SystemTime::now();
        if let Some(ts_peer_set) = self.cache.write().await.get_mut(ts.key()) {
            ts_peer_set.insert(p, current_time)
        } else {
            let mut map = AHashMap::new();
            map.insert(p, current_time);
            let key = ts.key().clone();
            self.cache.write().await.put(key, map);
            None
        }
    }

    pub async fn get_peers(&self, ts: Tipset) -> Option<Vec<PeerId>> {
        let mut v: Vec<PeerId> = vec![];

        self.cache
            .read()
            .await
            .peek(ts.key())?
            .keys()
            .for_each(|value| v.push(value.clone()));

        Some(v)
    }
}
