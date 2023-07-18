// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{num::NonZeroUsize, sync::Arc};

use crate::blocks::{Tipset, TipsetKeys};
use crate::metrics;
use crate::shim::clock::ChainEpoch;
use fvm_ipld_blockstore::Blockstore;
use itertools::Itertools;
use lru::LruCache;
use nonzero_ext::nonzero;
use parking_lot::Mutex;

use crate::chain::Error;

const DEFAULT_TIPSET_CACHE_SIZE: NonZeroUsize = nonzero!(8192usize);

type TipsetCache = Mutex<LruCache<TipsetKeys, Arc<Tipset>>>;

/// Keeps look-back tipsets in cache at a given interval `skip_length` and can
/// be used to look-back at the chain to retrieve an old tipset.
pub struct ChainIndex<DB> {
    /// `Arc` reference tipset cache.
    ts_cache: TipsetCache,

    /// `Blockstore` pointer needed to load tipsets from cold storage.
    db: Arc<DB>,
}

#[derive(Debug, Clone, Copy)]
// Methods for resolving fetches of null tipsets.
// Imagine epoch 10 is null but epoch 9 and 11 exist. If epoch we request epoch
// 10, should 9 or 11 be returned?
pub enum ResolveNullTipset {
    TakeYounger,
    TakeOlder,
}

impl<DB: Blockstore> ChainIndex<DB> {
    pub(in crate::chain) fn new(db: Arc<DB>) -> Self {
        let ts_cache = Mutex::new(LruCache::new(DEFAULT_TIPSET_CACHE_SIZE));
        Self { ts_cache, db }
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

    /// Find tipset at epoch `to` in the chain indicated by `from`. The `from`
    /// tipset's epoch must not be smaller than `to`.
    ///
    /// # Why pass in the `from` argument?
    ///
    /// Imagine the database contains five tipsets and a genesis block in this
    /// configuration:
    ///
    /// ```text
    ///           ┌───────┐  ┌────────┐  ┌────────┐
    /// Genesis◄──┤Epoch 1◄──┤Epoch 2A◄──┤Epoch 3A│
    ///           └───▲───┘  └────────┘  └────────┘
    ///               │      ┌────────┐  ┌────────┐
    ///               └──────┤Epoch 2B◄──┤Epoch 3B│
    ///                      └────────┘  └────────┘
    /// ```
    ///
    /// Here we have a fork in the chain and it is ambiguous which tipset to
    /// load when epoch 2 is requested. The ambiguity is solved by passing in a
    /// younger tipset (higher epoch) from which has the desired tipset as an
    /// ancestor.
    /// Calling `get_tipset_by_height(epoch_3a, 2)` will return `Epoch 2A`.
    /// Calling `get_tipset_by_height(epoch_3b, 2)` will return `Epoch 2B`.
    ///
    /// # What happens when a null tipset is requested?
    ///
    /// ```text
    ///           ┌───────┐          ┌───────┐  ┌───────┐
    /// Genesis◄──┤Epoch 1│   Null   │Epoch 3◄──┤Epoch 4│
    ///           └───▲───┘          └───┬───┘  └───────┘
    ///               │                  │
    ///               └──────────────────┘
    /// ```
    /// If the requested epoch points to a null tipset, there are two options:
    /// Pick the nearest older tipset or pick the nearest younger tipset.
    /// Requesting epoch 2 with `ResolveNullTipset::TakeYounger` will return
    /// epoch 3. Requesting with `ResolveNullTipset::TakeOlder` will return
    /// epoch 1.
    pub fn tipset_by_height(
        &self,
        to: ChainEpoch,
        from: Arc<Tipset>,
        resolve: ResolveNullTipset,
    ) -> Result<Arc<Tipset>, Error> {
        if to == 0 {
            return Ok(Arc::new(Tipset::from(from.genesis(&self.db)?)));
        }
        if to > from.epoch() {
            return Err(Error::Other(
                "Looking for tipset with height greater than start point".to_string(),
            ));
        }

        for (child, parent) in self.chain(from).tuple_windows() {
            if to == child.epoch() {
                return Ok(child);
            }
            if to < parent.epoch() {
                match resolve {
                    ResolveNullTipset::TakeOlder => return Ok(parent),
                    ResolveNullTipset::TakeYounger => return Ok(child),
                }
            }
        }
        Err(Error::Other(
            "Tipset with epoch {to} does not exist".to_string(),
        ))
    }

    /// Iterate from the given tipset to genesis. Missing tipsets cut the chain short.
    pub fn chain(&self, from: Arc<Tipset>) -> impl Iterator<Item = Arc<Tipset>> + '_ {
        itertools::unfold(Some(from), move |tipset| {
            tipset.take().map(|child| {
                *tipset = self.load_tipset(child.parents()).ok();
                child
            })
        })
    }
}
