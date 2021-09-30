// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{tipset_from_keys, Error, TipsetCache};
use async_std::sync::RwLock;
use blocks::{Tipset, TipsetKeys};
use clock::ChainEpoch;
use ipld_blockstore::BlockStore;
use lru::LruCache;
use std::sync::Arc;

const DEFAULT_CHAIN_INDEX_CACHE_SIZE: usize = 32 << 10;

/// Configuration which sets the length of tipsets to skip in between each cached entry.
const SKIP_LENGTH: ChainEpoch = 20;

/// Lookback entry to cache in the `ChainIndex`. Stores all relevant info when doing lookbacks.
#[derive(Clone, PartialEq, Debug)]
pub(crate) struct LookbackEntry {
    tipset: Arc<Tipset>,
    parent_height: ChainEpoch,
    target_height: ChainEpoch,
    target: TipsetKeys,
}

/// Keeps lookback tipsets in cache at a given interval `skip_length` and can be used to lookback
/// at the chain to retrieve an old tipset.
pub(crate) struct ChainIndex<BS> {
    /// Cache of lookback entries to speed up lookup.
    skip_cache: RwLock<LruCache<TipsetKeys, Arc<LookbackEntry>>>,

    /// `Arc` reference tipset cache.
    ts_cache: Arc<TipsetCache>,

    /// BlockStore pointer needed to load tipsets from cold storage.
    db: Arc<BS>,
}

impl<BS> ChainIndex<BS>
where
    BS: BlockStore + Send + Sync + 'static,
{
    pub(crate) fn new(ts_cache: Arc<TipsetCache>, db: Arc<BS>) -> Self {
        Self {
            skip_cache: RwLock::new(LruCache::new(DEFAULT_CHAIN_INDEX_CACHE_SIZE)),
            ts_cache,
            db,
        }
    }

    async fn load_tipset(&self, tsk: &TipsetKeys) -> Result<Arc<Tipset>, Error> {
        tipset_from_keys(self.ts_cache.as_ref(), self.db.as_ref(), tsk).await
    }

    /// Loads tipset at `to` [ChainEpoch], loading from sparse cache and/or loading parents
    /// from the blockstore.
    pub(crate) async fn get_tipset_by_height(
        &self,
        from: Arc<Tipset>,
        to: ChainEpoch,
    ) -> Result<Arc<Tipset>, Error> {
        if from.epoch() - to <= SKIP_LENGTH {
            return self.walk_back(from, to).await;
        }

        let rounded = self.round_down(from).await?;

        let mut cur = rounded.key().clone();
        loop {
            let entry = self.skip_cache.write().await.get(&cur).cloned();
            let lbe = if let Some(cached) = entry {
                cached
            } else {
                self.fill_cache(std::mem::take(&mut cur)).await?
            };

            if lbe.tipset.epoch() == to || lbe.parent_height < to {
                return Ok(lbe.tipset.clone());
            } else if to > lbe.target_height {
                return self.walk_back(lbe.tipset.clone(), to).await;
            }

            cur = lbe.target.clone();
        }
    }

    /// Walks back from the tipset, ignoring the cached entries.
    /// This should only be used when the cache is checked to be invalidated.
    pub(crate) async fn get_tipset_by_height_without_cache(
        &self,
        from: Arc<Tipset>,
        to: ChainEpoch,
    ) -> Result<Arc<Tipset>, Error> {
        self.walk_back(from, to).await
    }

    /// Fills cache with lookback entry, and returns inserted entry.
    async fn fill_cache(&self, tsk: TipsetKeys) -> Result<Arc<LookbackEntry>, Error> {
        let tipset = self.load_tipset(&tsk).await?;

        if tipset.epoch() == 0 {
            return Ok(Arc::new(LookbackEntry {
                tipset,
                parent_height: 0,
                target_height: Default::default(),
                target: Default::default(),
            }));
        }

        let parent = self.load_tipset(tipset.parents()).await?;
        let r_height = self.round_height(tipset.epoch()) - SKIP_LENGTH;

        let parent_epoch = parent.epoch();
        let skip_target = if parent.epoch() < r_height {
            parent
        } else {
            self.walk_back(parent, r_height).await?
        };

        let lbe = Arc::new(LookbackEntry {
            tipset,
            parent_height: parent_epoch,
            target_height: skip_target.epoch(),
            target: skip_target.key().clone(),
        });

        self.skip_cache.write().await.put(tsk.clone(), lbe.clone());
        Ok(lbe)
    }

    /// Rounds height epoch to nearest sparse cache index epoch.
    fn round_height(&self, height: ChainEpoch) -> ChainEpoch {
        (height / SKIP_LENGTH) * SKIP_LENGTH
    }

    /// Gets the closest rounded sparse index and returns the loaded tipset at that index.
    async fn round_down(&self, ts: Arc<Tipset>) -> Result<Arc<Tipset>, Error> {
        let target = self.round_height(ts.epoch());

        self.walk_back(ts, target).await
    }

    /// Load parent tipsets until the `to` [ChainEpoch].
    async fn walk_back(&self, from: Arc<Tipset>, to: ChainEpoch) -> Result<Arc<Tipset>, Error> {
        if to > from.epoch() {
            return Err(Error::Other(
                "Looking for tipset with height greater than start point".to_string(),
            ));
        }

        if to == from.epoch() {
            return Ok(from);
        }

        let mut ts = from;
        loop {
            let pts = self.load_tipset(ts.parents()).await?;

            if to > pts.epoch() {
                // Pts is lower than to epoch, return the tipset above that height
                return Ok(ts);
            }

            if to == pts.epoch() {
                return Ok(pts);
            }
            ts = pts;
        }
    }
}
