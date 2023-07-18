// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{num::NonZeroUsize, sync::Arc};

use crate::blocks::{Tipset, TipsetKeys};
use crate::metrics;
use crate::shim::clock::ChainEpoch;
use fvm_ipld_blockstore::Blockstore;
use lru::LruCache;
use nonzero_ext::nonzero;
use parking_lot::Mutex;

use crate::chain::Error;

const DEFAULT_TIPSET_CACHE_SIZE: NonZeroUsize = nonzero!(8192usize);

const DEFAULT_CHAIN_INDEX_CACHE_SIZE: NonZeroUsize = nonzero!(32usize << 10);

/// Configuration which sets the length of tipsets to skip in between each
/// cached entry.
const SKIP_LENGTH: ChainEpoch = 20;

/// `Lookback` entry to cache in the `ChainIndex`. Stores all relevant info when
/// doing `lookbacks`.
#[derive(Clone, PartialEq, Debug)]
struct LookbackEntry {
    tipset: Arc<Tipset>,
    parent_height: ChainEpoch,
    target_height: ChainEpoch,
    target: TipsetKeys,
}

type TipsetCache = Mutex<LruCache<TipsetKeys, Arc<Tipset>>>;

/// Keeps look-back tipsets in cache at a given interval `skip_length` and can
/// be used to look-back at the chain to retrieve an old tipset.
pub struct ChainIndex<DB> {
    /// Cache of look-back entries to speed up lookup.
    skip_cache: Mutex<LruCache<TipsetKeys, Arc<LookbackEntry>>>,

    /// `Arc` reference tipset cache.
    ts_cache: TipsetCache,

    /// `Blockstore` pointer needed to load tipsets from cold storage.
    db: Arc<DB>,
}

impl<DB: Blockstore> ChainIndex<DB> {
    pub(in crate::chain) fn new(db: Arc<DB>) -> Self {
        let ts_cache = Mutex::new(LruCache::new(DEFAULT_TIPSET_CACHE_SIZE));
        Self {
            skip_cache: Mutex::new(LruCache::new(DEFAULT_CHAIN_INDEX_CACHE_SIZE)),
            ts_cache,
            db,
        }
    }

    /// Loads a tipset from memory given the tipset keys and cache.
    pub fn load_tipset(&self, tsk: &TipsetKeys) -> Result<Arc<Tipset>, Error> {
        if let Some(ts) = self.ts_cache.lock().get(tsk) {
            metrics::LRU_CACHE_HIT
                .with_label_values(&[metrics::values::TIPSET])
                .inc();
            return Ok(ts.clone());
        }

        let ts = Arc::new(
            Tipset::load(&self.db, tsk)?.ok_or(Error::NotFound(String::from("Key for header")))?,
        );
        self.ts_cache.lock().put(tsk.clone(), ts.clone());
        metrics::LRU_CACHE_MISS
            .with_label_values(&[metrics::values::TIPSET])
            .inc();
        Ok(ts)
    }

    /// Loads tipset at `to` [`ChainEpoch`], loading from sparse cache and/or
    /// loading parents from the `blockstore`.
    pub(in crate::chain) fn get_tipset_by_height(
        &self,
        from: Arc<Tipset>,
        to: ChainEpoch,
    ) -> Result<Arc<Tipset>, Error> {
        if to == 0 {
            return Ok(Arc::new(Tipset::from(from.genesis(&self.db)?)));
        }
        if from.epoch() - to <= SKIP_LENGTH {
            return self.walk_back(from, to);
        }
        let rounded = self.round_down(from)?;

        let mut cur = rounded.key().clone();

        loop {
            let entry = self.skip_cache.lock().get(&cur).cloned();
            let lbe = if let Some(cached) = entry {
                metrics::LRU_CACHE_HIT
                    .with_label_values(&[metrics::values::SKIP])
                    .inc();
                cached
            } else {
                metrics::LRU_CACHE_MISS
                    .with_label_values(&[metrics::values::SKIP])
                    .inc();
                self.fill_cache(std::mem::take(&mut cur))?
            };

            if lbe.tipset.epoch() == to || lbe.parent_height < to {
                return Ok(lbe.tipset.clone());
            } else if to > lbe.target_height {
                return self.walk_back(lbe.tipset.clone(), to);
            }

            cur = lbe.target.clone();
        }
    }

    /// Walks back from the tipset, ignoring the cached entries.
    /// This should only be used when the cache is checked to be invalidated.
    pub(in crate::chain) fn get_tipset_by_height_without_cache(
        &self,
        from: Arc<Tipset>,
        to: ChainEpoch,
    ) -> Result<Arc<Tipset>, Error> {
        self.walk_back(from, to)
    }

    /// Fills cache with look-back entry, and returns inserted entry.
    fn fill_cache(&self, tsk: TipsetKeys) -> Result<Arc<LookbackEntry>, Error> {
        let tipset = self.load_tipset(&tsk)?;

        if tipset.epoch() == 0 {
            return Ok(Arc::new(LookbackEntry {
                tipset,
                parent_height: 0,
                target_height: Default::default(),
                target: Default::default(),
            }));
        }

        let parent = self.load_tipset(tipset.parents())?;
        let r_height = self.round_height(tipset.epoch()) - SKIP_LENGTH;

        let parent_epoch = parent.epoch();
        let skip_target = if parent.epoch() < r_height {
            parent
        } else {
            self.walk_back(parent, r_height)?
        };

        let lbe = Arc::new(LookbackEntry {
            tipset,
            parent_height: parent_epoch,
            target_height: skip_target.epoch(),
            target: skip_target.key().clone(),
        });

        self.skip_cache.lock().put(tsk, lbe.clone());
        Ok(lbe)
    }

    /// Rounds height epoch to nearest sparse cache index epoch.
    fn round_height(&self, height: ChainEpoch) -> ChainEpoch {
        (height / SKIP_LENGTH) * SKIP_LENGTH
    }

    /// Gets the closest rounded sparse index and returns the loaded tipset at
    /// that index.
    fn round_down(&self, ts: Arc<Tipset>) -> Result<Arc<Tipset>, Error> {
        let target = self.round_height(ts.epoch());

        self.walk_back(ts, target)
    }

    /// Load parent tipsets until the `to` [`ChainEpoch`].
    fn walk_back(&self, from: Arc<Tipset>, to: ChainEpoch) -> Result<Arc<Tipset>, Error> {
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
            let pts = self.load_tipset(ts.parents())?;

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
