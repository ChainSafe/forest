// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::num::NonZeroUsize;

use crate::beacon::{BeaconEntry, IGNORE_DRAND};
use crate::blocks::{Tipset, TipsetKey};
use crate::chain::Error;
use crate::metrics;
use crate::shim::clock::ChainEpoch;
use crate::utils::cache::SizeTrackingLruCache;
use fvm_ipld_blockstore::Blockstore;
use itertools::Itertools;
use nonzero_ext::nonzero;
use num::Integer;

const DEFAULT_TIPSET_CACHE_SIZE: NonZeroUsize = nonzero!(2880_usize);

type TipsetCache = SizeTrackingLruCache<TipsetKey, Tipset>;

type TipsetHeightCache = SizeTrackingLruCache<ChainEpoch, TipsetKey>;

type IsTipsetFinalizedFn = Box<dyn Fn(&Tipset) -> bool + Send + Sync>;

/// Keeps look-back tipsets in cache at a given interval `skip_length` and can
/// be used to look-back at the chain to retrieve an old tipset.
pub struct ChainIndex<DB> {
    /// tipset key to tipset mappings.
    ts_cache: TipsetCache,
    /// epoch to tipset key mappings.
    ts_height_cache: TipsetHeightCache,
    /// `Blockstore` pointer needed to load tipsets from cold storage.
    db: DB,
    /// check whether a tipset is finalized
    is_tipset_finalized: Option<IsTipsetFinalizedFn>,
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
    pub fn new(db: DB) -> Self {
        let ts_cache =
            SizeTrackingLruCache::new_with_metrics("tipset".into(), DEFAULT_TIPSET_CACHE_SIZE);
        let ts_height_cache: SizeTrackingLruCache<ChainEpoch, TipsetKey> =
            SizeTrackingLruCache::new_with_metrics(
                "tipset_by_height".into(),
                // 1048576 * 20 = 20971520 which is sufficient for mainnet
                // Maximum ~32MiB RAM usage
                nonzero!(1048576_usize),
            );
        Self {
            ts_cache,
            ts_height_cache,
            db,
            is_tipset_finalized: None,
        }
    }

    pub fn with_is_tipset_finalized(mut self, f: IsTipsetFinalizedFn) -> Self {
        self.is_tipset_finalized = Some(f);
        self
    }

    pub fn db(&self) -> &DB {
        &self.db
    }

    /// Loads a tipset from memory given the tipset keys and cache. Semantically
    /// identical to [`Tipset::load`] but the result is cached.
    pub fn load_tipset(&self, tsk: &TipsetKey) -> Result<Option<Tipset>, Error> {
        crate::def_is_env_truthy!(cache_disabled, "FOREST_TIPSET_CACHE_DISABLED");
        if !cache_disabled()
            && let Some(ts) = self.ts_cache.get_cloned(tsk)
        {
            metrics::LRU_CACHE_HIT
                .get_or_create(&metrics::values::TIPSET)
                .inc();
            return Ok(Some(ts));
        }

        let ts_opt = Tipset::load(&self.db, tsk)?;
        if !cache_disabled()
            && let Some(ts) = &ts_opt
        {
            self.ts_cache.push(tsk.clone(), ts.clone());
            metrics::LRU_CACHE_MISS
                .get_or_create(&metrics::values::TIPSET)
                .inc();
        }

        Ok(ts_opt)
    }

    /// Loads a tipset from memory given the tipset keys and cache.
    /// This calls fails if the tipset is missing or invalid. Semantically
    /// identical to [`Tipset::load_required`] but the result is cached.
    pub fn load_required_tipset(&self, tsk: &TipsetKey) -> Result<Tipset, Error> {
        self.load_tipset(tsk)?
            .ok_or_else(|| Error::NotFound("Key for header".into()))
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
        mut from: Tipset,
        resolve: ResolveNullTipset,
    ) -> Result<Tipset, Error> {
        use crate::shim::policy::policy_constants::CHAIN_FINALITY;

        // use `20` as checkpoint interval to match Lotus:
        // <https://github.com/filecoin-project/lotus/blob/v1.35.1/chain/store/index.go#L52>
        const CHECKPOINT_INTERVAL: ChainEpoch = 20;
        fn next_checkpoint(epoch: ChainEpoch) -> ChainEpoch {
            epoch - epoch.mod_floor(&CHECKPOINT_INTERVAL) + CHECKPOINT_INTERVAL
        }
        fn is_checkpoint(epoch: ChainEpoch) -> bool {
            epoch.mod_floor(&CHECKPOINT_INTERVAL) == 0
        }

        let from_epoch = from.epoch();

        let mut checkpoint_from_epoch = to;
        while checkpoint_from_epoch < from_epoch {
            if let Some(checkpoint_from_key) =
                self.ts_height_cache.get_cloned(&checkpoint_from_epoch)
                && let Ok(Some(checkpoint_from)) = self.load_tipset(&checkpoint_from_key)
            {
                from = checkpoint_from;
                break;
            }
            checkpoint_from_epoch = next_checkpoint(checkpoint_from_epoch);
        }

        if to == 0 {
            return Ok(Tipset::from(from.genesis(&self.db)?));
        }
        if to > from.epoch() {
            return Err(Error::Other(format!(
                "looking for tipset with height greater than start point, req: {to}, head: {from}",
                from = from.epoch()
            )));
        }

        let from_epoch = from.epoch();
        let is_finalized = |ts: &Tipset| {
            if let Some(is_finalized_fn) = &self.is_tipset_finalized {
                is_finalized_fn(ts)
            } else {
                ts.epoch() <= from_epoch - CHAIN_FINALITY
            }
        };
        for (child, parent) in from.chain(&self.db).tuple_windows() {
            // update cache only when child is finalized.
            if is_checkpoint(child.epoch()) && is_finalized(&child) {
                self.ts_height_cache
                    .push(child.epoch(), child.key().clone());
            }

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
        Err(Error::Other(format!(
            "Tipset with epoch={to} does not exist"
        )))
    }

    /// Finds the latest beacon entry given a tipset up to 20 tipsets behind
    pub fn latest_beacon_entry(&self, tipset: Tipset) -> Result<BeaconEntry, Error> {
        for ts in tipset.chain(&self.db).take(20) {
            if let Some(entry) = ts.min_ticket_block().beacon_entries.last() {
                return Ok(entry.clone());
            }
            if ts.epoch() == 0 {
                return Err(Error::Other(
                    "made it back to genesis block without finding beacon entry".to_owned(),
                ));
            }
        }

        if *IGNORE_DRAND {
            return Ok(BeaconEntry::new(0, vec![9; 16]));
        }

        Err(Error::Other(
            "Found no beacon entries in the 20 latest tipsets".to_owned(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    };

    use super::*;
    use crate::blocks::{CachingBlockHeader, RawBlockHeader};
    use crate::db::MemoryDB;
    use crate::utils::db::CborStoreExt;

    fn persist_tipset(tipset: &Tipset, db: &impl Blockstore) {
        for block in tipset.block_headers() {
            db.put_cbor_default(block).unwrap();
        }
    }

    fn genesis_tipset() -> Tipset {
        Tipset::from(CachingBlockHeader::default())
    }

    fn tipset_child(parent: &Tipset, epoch: ChainEpoch) -> Tipset {
        // Use a static counter to give all tipsets a unique timestamp
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        Tipset::from(CachingBlockHeader::new(RawBlockHeader {
            parents: parent.key().clone(),
            epoch,
            timestamp: n,
            ..Default::default()
        }))
    }

    #[test]
    fn get_null_tipset() {
        let db = Arc::new(MemoryDB::default());
        let genesis = genesis_tipset();
        let epoch1 = tipset_child(&genesis, 1);
        let epoch3 = tipset_child(&epoch1, 3);
        let epoch4 = tipset_child(&epoch3, 4);
        persist_tipset(&genesis, &db);
        persist_tipset(&epoch1, &db);
        persist_tipset(&epoch3, &db);
        persist_tipset(&epoch4, &db);

        let index = ChainIndex::new(db);
        // epoch 2 is null. ResolveNullTipset decided whether to return epoch 1 or epoch 3
        assert_eq!(
            index
                .tipset_by_height(2, epoch4.clone(), ResolveNullTipset::TakeOlder)
                .unwrap(),
            epoch1
        );

        assert_eq!(
            index
                .tipset_by_height(2, epoch4, ResolveNullTipset::TakeNewer)
                .unwrap(),
            epoch3
        );
    }

    #[test]
    fn get_different_branches() {
        let db = Arc::new(MemoryDB::default());
        let genesis = genesis_tipset();
        let epoch1 = tipset_child(&genesis, 1);

        let epoch2a = tipset_child(&epoch1, 2);
        let epoch3a = tipset_child(&epoch2a, 3);

        let epoch2b = tipset_child(&epoch1, 2);
        let epoch3b = tipset_child(&epoch2b, 3);

        persist_tipset(&genesis, &db);
        persist_tipset(&epoch1, &db);
        persist_tipset(&epoch2a, &db);
        persist_tipset(&epoch3a, &db);
        persist_tipset(&epoch2b, &db);
        persist_tipset(&epoch3b, &db);

        let index = ChainIndex::new(db);
        // The chain as forked, epoch 2 and 3 are ambiguous
        assert_eq!(
            index
                .tipset_by_height(2, epoch3a, ResolveNullTipset::TakeOlder)
                .unwrap(),
            epoch2a
        );

        assert_eq!(
            index
                .tipset_by_height(2, epoch3b, ResolveNullTipset::TakeOlder)
                .unwrap(),
            epoch2b
        );
    }
}
