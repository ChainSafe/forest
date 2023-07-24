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
    db: DB,
}

#[derive(Debug, Clone, Copy)]
/// Methods for resolving fetches of null tipsets.
/// Imagine epoch 10 is null but epoch 9 and 11 exist. If epoch we request epoch
/// 10, should 9 or 11 be returned?
pub enum ResolveNullTipset {
    TakeNewer,
    TakeOlder,
}

impl<DB: Blockstore> ChainIndex<DB> {
    pub(in crate::chain) fn new(db: DB) -> Self {
        let ts_cache = Mutex::new(LruCache::new(DEFAULT_TIPSET_CACHE_SIZE));
        Self { ts_cache, db }
    }

    /// Loads a tipset from memory given the tipset keys and cache. Semantically
    /// identical to [`Tipset::load`] but the result is cached.
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

    /// Find tipset at epoch `to` in the chain of ancestors starting at `from`.
    /// If the tipset is _not_ in the chain of ancestors (i.e., if the `to`
    /// epoch is higher than `from.epoch()`), an error will be returned.
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
    /// Calling `get_tipset_by_height(2, epoch_3a)` will return `Epoch 2A`.
    /// Calling `get_tipset_by_height(2, epoch_3b)` will return `Epoch 2B`.
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
    /// Requesting epoch 2 with [`ResolveNullTipset::TakeNewer`] will return
    /// epoch 3. Requesting with [`ResolveNullTipset::TakeOlder`] will return
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
            if to > parent.epoch() {
                // We're at a point where child.epoch() > x > parent.epoch().
                match resolve {
                    ResolveNullTipset::TakeOlder => return Ok(parent),
                    ResolveNullTipset::TakeNewer => return Ok(child),
                }
            }
        }
        Err(Error::Other(
            "Tipset with epoch={to} does not exist".to_string(),
        ))
    }

    /// Iterate from the given tipset to genesis. Missing tipsets cut the chain
    /// short. Semantically identical to [`Tipset::chain`] but the results are
    /// cached.
    pub fn chain(&self, from: Arc<Tipset>) -> impl Iterator<Item = Arc<Tipset>> + '_ {
        itertools::unfold(Some(from), move |tipset| {
            tipset.take().map(|child| {
                *tipset = self.load_tipset(child.parents()).ok();
                child
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;
    use crate::blocks::BlockHeader;
    use crate::db::MemoryDB;
    use crate::utils::db::CborStoreExt;

    fn persist_tipset(tipset: &Tipset, db: &impl Blockstore) {
        for block in tipset.blocks() {
            db.put_cbor_default(block).unwrap();
        }
    }

    fn genesis_tipset() -> Tipset {
        Tipset::from(BlockHeader::default())
    }

    fn tipset_child(parent: &Tipset, epoch: ChainEpoch) -> Tipset {
        // Use a static counter to give all tipsets a unique timestamp
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        Tipset::from(
            BlockHeader::builder()
                .parents(parent.key().clone())
                .epoch(epoch)
                .timestamp(n)
                .build()
                .unwrap(),
        )
    }

    #[test]
    fn get_null_tipset() {
        let db = Arc::new(MemoryDB::default());
        let gen = genesis_tipset();
        let epoch1 = tipset_child(&gen, 1);
        let epoch3 = tipset_child(&epoch1, 3);
        let epoch4 = tipset_child(&epoch3, 4);
        persist_tipset(&gen, &db);
        persist_tipset(&epoch1, &db);
        persist_tipset(&epoch3, &db);
        persist_tipset(&epoch4, &db);

        let index = ChainIndex::new(db);
        // epoch 2 is null. ResolveNullTipset decided whether to return epoch 1 or epoch 3
        assert_eq!(
            index
                .tipset_by_height(2, Arc::new(epoch4.clone()), ResolveNullTipset::TakeOlder)
                .unwrap()
                .as_ref(),
            &epoch1
        );

        assert_eq!(
            index
                .tipset_by_height(2, Arc::new(epoch4), ResolveNullTipset::TakeNewer)
                .unwrap()
                .as_ref(),
            &epoch3
        );
    }

    #[test]
    fn get_different_branches() {
        let db = Arc::new(MemoryDB::default());
        let gen = genesis_tipset();
        let epoch1 = tipset_child(&gen, 1);

        let epoch2a = tipset_child(&epoch1, 2);
        let epoch3a = tipset_child(&epoch2a, 3);

        let epoch2b = tipset_child(&epoch1, 2);
        let epoch3b = tipset_child(&epoch2b, 3);

        persist_tipset(&gen, &db);
        persist_tipset(&epoch1, &db);
        persist_tipset(&epoch2a, &db);
        persist_tipset(&epoch3a, &db);
        persist_tipset(&epoch2b, &db);
        persist_tipset(&epoch3b, &db);

        let index = ChainIndex::new(db);
        // The chain as forked, epoch 2 and 3 are ambiguous
        assert_eq!(
            index
                .tipset_by_height(2, Arc::new(epoch3a), ResolveNullTipset::TakeOlder)
                .unwrap()
                .as_ref(),
            &epoch2a
        );

        assert_eq!(
            index
                .tipset_by_height(2, Arc::new(epoch3b), ResolveNullTipset::TakeOlder)
                .unwrap()
                .as_ref(),
            &epoch2b
        );
    }
}
