// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::num::NonZeroUsize;
use std::sync::atomic::{self, AtomicI64};

use crate::beacon::{BeaconEntry, IGNORE_DRAND};
use crate::blocks::{Tipset, TipsetKey};
use crate::chain::Error;
use crate::db::{DbImpl, EthMappingsStore};
use crate::prelude::*;
use crate::shim::clock::ChainEpoch;
use crate::utils::cache::SizeTrackingCache;
use nonzero_ext::nonzero;
use num::Integer;
use tracing::{info, warn};

const DEFAULT_TIPSET_CACHE_SIZE: NonZeroUsize = nonzero!(2880_usize * 3); // 3-day-worth epochs, maximum ~50MiB
// use `20` as checkpoint interval to match Lotus:
// <https://github.com/filecoin-project/lotus/blob/v1.35.1/chain/store/index.go#L52>
const TIPSET_LOOKUP_CHECKPOINT_INTERVAL: ChainEpoch = 20;

type TipsetCache = SizeTrackingCache<TipsetKey, Tipset>;

type IsEpochFinalizedFn = Arc<dyn Fn(ChainEpoch) -> bool + Send + Sync>;

/// Keeps look-back tipsets in cache at a given interval `skip_length` and can
/// be used to look-back at the chain to retrieve an old tipset.
pub struct ChainIndex {
    /// tipset key to tipset mappings.
    ts_cache: TipsetCache,
    /// `Blockstore` pointer needed to load tipsets from cold storage.
    db: DbImpl,
    /// Genesis tipset
    genesis: Tipset,
    /// check whether an epoch is finalized
    is_epoch_finalized: Option<IsEpochFinalizedFn>,
    /// Newest checkpoint epoch verified by [`Self::update_tipset_lookup_for_finalized_head`],
    /// letting it skip the ancestor walk until the boundary advances.
    last_verified_checkpoint: Arc<AtomicI64>,
}

impl ShallowClone for ChainIndex {
    fn shallow_clone(&self) -> Self {
        Self {
            ts_cache: self.ts_cache.shallow_clone(),
            db: self.db.shallow_clone(),
            genesis: self.genesis.shallow_clone(),
            is_epoch_finalized: self.is_epoch_finalized.clone(),
            last_verified_checkpoint: self.last_verified_checkpoint.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
/// Methods for resolving fetches of null tipsets.
/// Imagine epoch 10 is null but epoch 9 and 11 exist. If epoch we request epoch
/// 10, should 9 or 11 be returned?
pub enum ResolveNullTipset {
    TakeNewer,
    TakeOlder,
    /// Return [`Error::NullRound`] instead of resolving to a neighboring tipset.
    Fail,
}

impl ChainIndex {
    pub fn new(db: impl Into<DbImpl>, genesis: Tipset) -> Self {
        assert!(genesis.epoch() == 0, "genesis tipset must be at epoch 0");
        let db = db.into();
        let ts_cache = SizeTrackingCache::new_with_metrics("tipset", DEFAULT_TIPSET_CACHE_SIZE);
        Self {
            ts_cache,
            db,
            genesis,
            is_epoch_finalized: None,
            last_verified_checkpoint: Arc::new(AtomicI64::new(-1)),
        }
    }

    pub fn with_is_epoch_finalized(mut self, f: IsEpochFinalizedFn) -> Self {
        self.is_epoch_finalized = Some(f);
        self
    }

    pub fn db(&self) -> &DbImpl {
        &self.db
    }

    pub fn db_owned(&self) -> DbImpl {
        self.db().shallow_clone()
    }

    pub fn genesis(&self) -> &Tipset {
        &self.genesis
    }

    /// Loads a tipset from memory given the tipset keys and cache. Semantically
    /// identical to [`Tipset::load`] but the result is cached.
    pub fn load_tipset(&self, tsk: &TipsetKey) -> Result<Option<Tipset>, Error> {
        crate::def_is_env_truthy!(cache_disabled, "FOREST_TIPSET_CACHE_DISABLED");
        if cache_disabled() {
            Ok(Tipset::load(&self.db, tsk)?)
        } else {
            enum TmpError {
                NotFound,
                LoadError(anyhow::Error),
            }
            match self.ts_cache.get_or_insert_with(tsk, || {
                Tipset::load(&self.db, tsk)
                    .map(|opt| opt.ok_or(TmpError::NotFound))
                    .map_err(TmpError::LoadError)
                    .flatten()
            }) {
                Ok(ts) => Ok(Some(ts)),
                Err(TmpError::NotFound) => Ok(None),
                Err(TmpError::LoadError(e)) => Err(e.into()),
            }
        }
    }

    /// Loads a tipset from memory given the tipset keys and cache.
    /// This calls fails if the tipset is missing or invalid. Semantically
    /// identical to [`Tipset::load_required`] but the result is cached.
    pub fn load_required_tipset(&self, tsk: &TipsetKey) -> Result<Tipset, Error> {
        self.load_tipset(tsk)?
            .ok_or_else(|| Error::NotFound("Key for header".into()))
    }

    /// Find tipset at epoch `to` in the chain of ancestors starting at `from`.
    ///
    /// Returns `Ok(Some(tipset))` when epoch `to` resolves. Returns `Ok(None)` if the ancestor
    /// walk completes without resolving `to` (for example missing parent tipsets). Returns `Err`
    /// if `to` is greater than `from.epoch()` or genesis lookup fails when `to` is zero.
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
    /// If the requested epoch points to a null tipset, there are three options:
    /// pick the nearest older tipset, pick the nearest younger tipset, or fail.
    /// Requesting epoch 2 with [`ResolveNullTipset::TakeNewer`] will return
    /// epoch 3, with [`ResolveNullTipset::TakeOlder`] will return epoch 1, and
    /// with [`ResolveNullTipset::Fail`] will return [`Error::NullRound`].
    pub fn tipset_by_height_blocking(
        &self,
        to: ChainEpoch,
        mut from: Tipset,
        resolve: ResolveNullTipset,
    ) -> Result<Option<Tipset>, Error> {
        use crate::shim::policy::policy_constants::CHAIN_FINALITY;

        crate::def_is_env_truthy!(lookup_table_disabled, "FOREST_TIPSET_LOOKUP_TABLE_DISABLED");

        if to == 0 {
            return Ok(Some(self.genesis.shallow_clone()));
        }

        let from_epoch = from.epoch();
        let is_epoch_finalized = |epoch: ChainEpoch| {
            if let Some(is_epoch_finalized) = &self.is_epoch_finalized {
                is_epoch_finalized(epoch)
            } else {
                epoch <= from_epoch - CHAIN_FINALITY
            }
        };

        let mut checkpoint_from_epoch = to;
        while !lookup_table_disabled()
            && checkpoint_from_epoch < from_epoch
            // unfinalized checkpoints are subject to change
            && is_epoch_finalized(checkpoint_from_epoch)
        {
            if let Ok(Some(checkpoint_from_key)) =
                self.db.tipset_key_by_epoch(checkpoint_from_epoch)
                && let Ok(Some(checkpoint_from)) = self.load_tipset(&checkpoint_from_key)
            {
                from = checkpoint_from;
                break;
            }
            checkpoint_from_epoch = Self::next_tipset_lookup_checkpoint(checkpoint_from_epoch);
        }

        if to > from.epoch() {
            return Err(Error::Other(format!(
                "looking for tipset with height greater than start point, req: {to}, head: {from}",
                from = from.epoch()
            )));
        } else if to == from.epoch() {
            return Ok(Some(from));
        }

        // Note: the walk deliberately does NOT populate the lookup table. `from` can be
        // any tipset (an unvalidated candidate chain from a peer, an RPC-supplied tipset
        // key), so tipsets encountered here are not guaranteed to be on the canonical
        // chain even at finalized epochs. The table is populated exclusively from the
        // canonical head: see [`Self::update_tipset_lookup_for_finalized_head`] and the
        // startup warmup in the daemon.
        for (child, parent) in from.chain(&self.db).tuple_windows() {
            if to == child.epoch() {
                return Ok(Some(child));
            }
            if to > parent.epoch() {
                // We're at a point where child.epoch() > x > parent.epoch().
                match resolve {
                    ResolveNullTipset::TakeOlder => return Ok(Some(parent)),
                    ResolveNullTipset::TakeNewer => return Ok(Some(child)),
                    ResolveNullTipset::Fail => return Err(Error::NullRound(to)),
                }
            }
        }
        Ok(None)
    }

    /// Non-blocking version of [`Self::tipset_by_height_blocking`]
    pub async fn tipset_by_height(
        &self,
        to: ChainEpoch,
        from: Tipset,
        resolve: ResolveNullTipset,
    ) -> Result<Option<Tipset>, Error> {
        let this = self.shallow_clone();
        tokio::task::spawn_blocking(move || this.tipset_by_height_blocking(to, from, resolve))
            .await?
    }

    /// Same as [`Self::tipset_by_height_blocking`], but errors if that would return `None`.
    /// This call can be expensive and blocking, use [`Self::load_required_tipset_by_height`]
    /// in async contexts to avoid exhausting Tokio worker threads.
    pub fn load_required_tipset_by_height_blocking(
        &self,
        to: ChainEpoch,
        from: Tipset,
        resolve: ResolveNullTipset,
    ) -> Result<Tipset, Error> {
        self.tipset_by_height_blocking(to, from, resolve)?
            .ok_or_else(|| Error::NotFound(format!("tipset at epoch {to}").into()))
    }

    /// Same as [`Self::tipset_by_height`], but errors if that would return `None`.
    pub async fn load_required_tipset_by_height(
        &self,
        to: ChainEpoch,
        from: Tipset,
        resolve: ResolveNullTipset,
    ) -> Result<Tipset, Error> {
        self.tipset_by_height(to, from, resolve)
            .await?
            .ok_or_else(|| Error::NotFound(format!("tipset at epoch {to}").into()))
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

    fn next_tipset_lookup_checkpoint(epoch: ChainEpoch) -> ChainEpoch {
        Self::prev_tipset_lookup_checkpoint(epoch) + TIPSET_LOOKUP_CHECKPOINT_INTERVAL
    }

    fn prev_tipset_lookup_checkpoint(epoch: ChainEpoch) -> ChainEpoch {
        epoch - epoch.mod_floor(&TIPSET_LOOKUP_CHECKPOINT_INTERVAL)
    }

    pub fn is_tipset_lookup_checkpoint(epoch: ChainEpoch) -> bool {
        // Genesis is not considered a checkpoint
        epoch > 0 && epoch.mod_floor(&TIPSET_LOOKUP_CHECKPOINT_INTERVAL) == 0
    }

    /// Verifies the lookup table down to the newest checkpoint epoch at or below
    /// `finalized_epoch` as the head advances. Only finalized ancestors of the head are ever
    /// recorded — a stale entry (e.g. an unfinalized head that later loses a tipset race)
    /// corrupts randomness and `tipset_cid` lookups in state computation and wedges sync.
    /// The walk only runs when the checkpoint boundary advances; other calls return
    /// immediately.
    pub fn update_tipset_lookup_for_finalized_head(
        &self,
        head: &Tipset,
        finalized_epoch: ChainEpoch,
    ) -> anyhow::Result<()> {
        let checkpoint_epoch = Self::prev_tipset_lookup_checkpoint(finalized_epoch);
        // No finalized checkpoint strictly below the head yet — nothing to record.
        if checkpoint_epoch <= 0 || checkpoint_epoch >= head.epoch() {
            return Ok(());
        }
        if checkpoint_epoch
            == self
                .last_verified_checkpoint
                .load(atomic::Ordering::Acquire)
        {
            return Ok(());
        }
        self.repair_tipset_lookup_window(head, head.epoch() - checkpoint_epoch, finalized_epoch)?;
        self.last_verified_checkpoint
            .store(checkpoint_epoch, atomic::Ordering::Release);
        Ok(())
    }

    /// Verifies the lookup table against the head's lineage over the last `lookback` epochs,
    /// repairing entries that disagree with it, and returns the number of wrong entries fixed.
    /// Missing entries are backfilled (at finalized epochs only) without counting, since an
    /// absent entry cannot have corrupted a computation. Any state computed from poisoned
    /// entries may be tainted; `StateManager::repair_tipset_lookup` pairs the repair with
    /// cache eviction.
    pub fn repair_tipset_lookup_window(
        &self,
        head: &Tipset,
        lookback: ChainEpoch,
        finalized_epoch: ChainEpoch,
    ) -> anyhow::Result<usize> {
        let stop_epoch = (head.epoch() - lookback).max(1);
        let mut n_repaired = 0;
        let mut ts = head.shallow_clone();
        while ts.epoch() >= stop_epoch {
            // Loading via `ts_cache`; successive walks revisit an almost identical window.
            // A missing parent is an error: the window cannot be verified with chain data
            // missing, and a success here would wrongly record the walk as complete
            // (verified checkpoint, clean repair scan).
            let parent = self.load_required_tipset(ts.parents()).with_context(|| {
                format!(
                    "missing parent tipset {} below epoch {}",
                    ts.parents(),
                    ts.epoch()
                )
            })?;
            // The head epoch is excluded: blocks for it may still arrive (tipset expansion).
            if Self::is_tipset_lookup_checkpoint(ts.epoch()) && ts.epoch() < head.epoch() {
                let epoch = ts.epoch();
                let prev = self
                    .db
                    .tipset_key_by_epoch(epoch)
                    .with_context(|| format!("failed to look up tipset key at epoch {epoch}"))?;
                match prev {
                    Some(ref tsk) if tsk == ts.key() => {}
                    // Absent entries are only backfilled at finalized epochs.
                    None if epoch > finalized_epoch => {}
                    prev => {
                        if let Some(prev) = prev {
                            warn!(
                                "Correcting tipset lookup at epoch {epoch}: expected {}, found {prev}",
                                ts.key(),
                            );
                            n_repaired += 1;
                        }
                        self.db.set_tipset_key_at_epoch(&ts).with_context(|| {
                            format!("failed to update tipset lookup at epoch {epoch}")
                        })?;
                    }
                }
            }
            n_repaired += Self::cleanup_stale_tipset_lookup_at_null_rounds(&self.db, &ts, &parent)?;
            ts = parent;
        }
        Ok(n_repaired)
    }

    /// Deletes stale lookup entries left behind by a reorg at null-round checkpoint epochs
    /// between the given head and its parent. A no-op in most cases; loading the parent also
    /// warms the tipset cache for the Eth APIs' `latest` tipset.
    pub fn cleanup_stale_lookup_at_new_head(&self, head: &Tipset) -> anyhow::Result<usize> {
        if let Some(parent) = self.load_tipset(head.parents())? {
            Self::cleanup_stale_tipset_lookup_at_null_rounds(&self.db, head, &parent)
        } else {
            Ok(0)
        }
    }

    /// Cleans up stale checkpoints at null rounds between the given tipset and its parent in case there's chain reorg.
    /// Returns the number of lookup entries being deleted.
    fn cleanup_stale_tipset_lookup_at_null_rounds(
        db: &impl EthMappingsStore,
        ts: &Tipset,
        parent: &Tipset,
    ) -> anyhow::Result<usize> {
        anyhow::ensure!(
            ts.parents() == parent.key(),
            "tipset keys do not match, `ts.parents()` should match `parent.key()`"
        );
        // Cleanup null lookup checkpoints on chain reorg
        let null_checkpoint_epochs = ((parent.epoch() + 1)..ts.epoch())
            .filter(|&epoch| Self::is_tipset_lookup_checkpoint(epoch))
            .collect_vec();
        let mut n_deleted = 0;
        for epoch in null_checkpoint_epochs {
            if db
                .tipset_key_by_epoch(epoch)
                .with_context(|| {
                    format!("db error: failed to dlookup tipset key at epoch {epoch}")
                })?
                .is_some()
            {
                db.delete_tipset_key_at_epoch(epoch).with_context(|| {
                    format!("db error: failed to delete tipset lookup at null epoch {epoch}")
                })?;
                info!("deleted tipset lookup at null epoch {epoch}");
                n_deleted += 1;
            }
        }
        Ok(n_deleted)
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::blocks::{CachingBlockHeader, RawBlockHeader};
    use crate::chain::store::ChainStore;
    use crate::db::{EthMappingsStore, MemoryDB};
    use crate::networks::ChainConfig;
    use crate::shim::address::Address;
    use crate::test_utils::dummy_ticket;
    use crate::utils::db::CborStoreExt;
    use std::sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    };

    pub fn persist_tipset(tipset: &Tipset, db: &impl Blockstore) {
        for block in tipset.block_headers() {
            db.put_cbor_default(block).unwrap();
        }
    }

    pub fn genesis_tipset() -> Tipset {
        Tipset::from(CachingBlockHeader::new(RawBlockHeader {
            ticket: dummy_ticket(0),
            ..Default::default()
        }))
    }

    pub fn tipset_child(parent: &Tipset, epoch: ChainEpoch) -> Tipset {
        // Use a static counter to give all tipsets a unique timestamp
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        Tipset::from(CachingBlockHeader::new(RawBlockHeader {
            parents: parent.key().clone(),
            ticket: dummy_ticket(n as u8),
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

        let index = ChainIndex::new(db, genesis);
        // epoch 2 is null. ResolveNullTipset decided whether to return epoch 1 or epoch 3
        assert_eq!(
            index
                .tipset_by_height_blocking(2, epoch4.clone(), ResolveNullTipset::TakeOlder)
                .unwrap()
                .expect("epoch 2 resolved"),
            epoch1
        );

        assert_eq!(
            index
                .tipset_by_height_blocking(2, epoch4, ResolveNullTipset::TakeNewer)
                .unwrap()
                .expect("epoch 2 resolved"),
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

        let index = ChainIndex::new(db, genesis);
        // The chain as forked, epoch 2 and 3 are ambiguous
        assert_eq!(
            index
                .tipset_by_height_blocking(2, epoch3a, ResolveNullTipset::TakeOlder)
                .unwrap()
                .expect("epoch 2 on branch a"),
            epoch2a
        );

        assert_eq!(
            index
                .tipset_by_height_blocking(2, epoch3b, ResolveNullTipset::TakeOlder)
                .unwrap()
                .expect("epoch 2 on branch b"),
            epoch2b
        );
    }

    /// Builds a chain with two competing tipsets at checkpoint epoch 20: `full` (canonical,
    /// two blocks), `partial` (one block, different min ticket), and a canonical chain up to
    /// epoch 30 built on `full`. Returns `(genesis, partial, full, head)`.
    fn chain_with_competing_tipsets_at_checkpoint(
        db: &Arc<MemoryDB>,
    ) -> (Tipset, Tipset, Tipset, Tipset) {
        let genesis = genesis_tipset();
        persist_tipset(&genesis, db);
        let mut prev = genesis.shallow_clone();
        for epoch in 1..20 {
            let ts = tipset_child(&prev, epoch);
            persist_tipset(&ts, db);
            prev = ts;
        }
        let full = Tipset::new([1, 2].map(|i| {
            CachingBlockHeader::new(RawBlockHeader {
                miner_address: Address::new_id(i),
                parents: prev.key().clone(),
                ticket: dummy_ticket(i as u8),
                epoch: 20,
                ..Default::default()
            })
        }))
        .unwrap();
        persist_tipset(&full, db);
        let partial = Tipset::from(full.block_headers().last());
        // Sanity for the incident scenario: chain randomness drawn at epoch 20
        // diverges between the two tipsets.
        assert_ne!(partial.min_ticket(), full.min_ticket());

        let mut head = full.shallow_clone();
        for epoch in 21..=30 {
            let ts = tipset_child(&head, epoch);
            persist_tipset(&ts, db);
            head = ts;
        }
        (genesis, partial, full, head)
    }

    /// Regression test for the 2026-07-12 mainnet incident: an unfinalized head persisted
    /// at a checkpoint epoch poisoned the table and wedged sync after losing a tipset race.
    #[test]
    fn tipset_by_height_does_not_resolve_stale_non_ancestor_checkpoint() {
        let db = Arc::new(MemoryDB::default());
        let (genesis, partial, full, head) = chain_with_competing_tipsets_at_checkpoint(&db);

        // Drive the real write path: the node's head lands on the partial tipset
        // at checkpoint epoch 20, then follows the canonical chain.
        let cs = ChainStore::new(
            db.clone(),
            Arc::new(ChainConfig::default()),
            genesis.shallow_clone(),
        )
        .unwrap();
        cs.set_heaviest_tipset(partial.shallow_clone()).unwrap();
        cs.set_heaviest_tipset(head.shallow_clone()).unwrap();

        // The abandoned head must not remain in the lookup table.
        assert_ne!(
            db.tipset_key_by_epoch(20).unwrap().as_ref(),
            Some(partial.key()),
            "unfinalized head must not be persisted in the lookup table",
        );

        // Reads consult the table once the epoch is considered finalized. In
        // production `is_epoch_finalized` is backed by the EC finality calculator,
        // which finalizes within ~20 epochs of head.
        let index = ChainIndex::new(db, genesis).with_is_epoch_finalized(Arc::new(|e| e <= 25));
        let resolved = index
            .tipset_by_height_blocking(20, head, ResolveNullTipset::TakeOlder)
            .unwrap()
            .expect("epoch 20 resolved");
        assert_eq!(
            resolved, full,
            "tipset_by_height must resolve the ancestor of `from`, not a stale checkpoint",
        );
    }

    #[test]
    fn update_tipset_lookup_for_finalized_head_writes_and_skips() {
        let db = Arc::new(MemoryDB::default());
        let (genesis, partial, full, head) = chain_with_competing_tipsets_at_checkpoint(&db);
        let index = ChainIndex::new(db.clone(), genesis);

        // Nothing to do while no checkpoint epoch is finalized.
        index
            .update_tipset_lookup_for_finalized_head(&head, 19)
            .unwrap();
        assert_eq!(db.tipset_key_by_epoch(20).unwrap(), None);

        // A checkpoint epoch at or above the head epoch is never recorded, even if
        // the finality calculator claims it is finalized.
        index
            .update_tipset_lookup_for_finalized_head(&full, 20)
            .unwrap();
        assert_eq!(db.tipset_key_by_epoch(20).unwrap(), None);

        // The finalized epoch itself is not a checkpoint: the newest checkpoint at
        // or below it is recorded, with the head's actual ancestor.
        index
            .update_tipset_lookup_for_finalized_head(&head, 25)
            .unwrap();
        assert_eq!(
            db.tipset_key_by_epoch(20).unwrap().as_ref(),
            Some(full.key())
        );

        // Calls at an already-verified boundary return without walking; corruption
        // within the finalized window is the repair path's job (see
        // `repair_tipset_lookup_window_fixes_poisoned_entry`).
        db.set_tipset_key_at_epoch(&partial).unwrap();
        index
            .update_tipset_lookup_for_finalized_head(&head, 25)
            .unwrap();
        assert_eq!(
            db.tipset_key_by_epoch(20).unwrap().as_ref(),
            Some(partial.key())
        );
    }

    #[test]
    fn repair_tipset_lookup_window_fixes_poisoned_entry() {
        let db = Arc::new(MemoryDB::default());
        let (genesis, partial, full, head) = chain_with_competing_tipsets_at_checkpoint(&db);
        let index = ChainIndex::new(db.clone(), genesis);

        // A stale entry, e.g. left behind by an older forest version or a reorg
        // deeper than the finality calculator's estimate.
        db.set_tipset_key_at_epoch(&partial).unwrap();
        let repaired = index.repair_tipset_lookup_window(&head, 900, 25).unwrap();
        assert_eq!(repaired, 1);
        assert_eq!(
            db.tipset_key_by_epoch(20).unwrap().as_ref(),
            Some(full.key())
        );

        // A clean table reports zero repairs (the missing finalized backfill at
        // epoch 20 was already written above and does not count as a repair).
        assert_eq!(
            index.repair_tipset_lookup_window(&head, 900, 25).unwrap(),
            0
        );

        // Missing entries are backfilled at finalized epochs only, and not counted.
        db.delete_tipset_key_at_epoch(20).unwrap();
        assert_eq!(
            index.repair_tipset_lookup_window(&head, 900, 19).unwrap(),
            0
        );
        assert_eq!(db.tipset_key_by_epoch(20).unwrap(), None);
        assert_eq!(
            index.repair_tipset_lookup_window(&head, 900, 25).unwrap(),
            0
        );
        assert_eq!(
            db.tipset_key_by_epoch(20).unwrap().as_ref(),
            Some(full.key())
        );
    }

    /// Pins the corruption vector of the 2026-07-12 incident: FVM chain randomness
    /// derives from the tipset the lookup table resolves, so a poisoned entry changes
    /// randomness — and thereby computed state roots — until the table is repaired.
    #[test]
    fn chain_randomness_diverges_on_poisoned_lookup_and_recovers_after_repair() {
        use crate::beacon::BeaconSchedule;
        use crate::state_manager::chain_rand::ChainRand;

        let db = Arc::new(MemoryDB::default());
        let (genesis, partial, _full, head) = chain_with_competing_tipsets_at_checkpoint(&db);
        let index =
            ChainIndex::new(db.clone(), genesis).with_is_epoch_finalized(Arc::new(|e| e <= 25));
        let rand = ChainRand::new(
            Arc::new(ChainConfig::default()),
            head.shallow_clone(),
            index.shallow_clone(),
            Arc::new(BeaconSchedule(vec![])),
        );

        // With no entry, the lookup falls back to the ancestor walk and resolves `full`.
        let canonical = rand.get_chain_randomness_blocking(20, false).unwrap();

        db.set_tipset_key_at_epoch(&partial).unwrap();
        let poisoned = rand.get_chain_randomness_blocking(20, false).unwrap();
        assert_ne!(
            canonical, poisoned,
            "a poisoned lookup entry must be observable in derived randomness for this test to be meaningful",
        );

        index.repair_tipset_lookup_window(&head, 900, 25).unwrap();
        assert_eq!(
            rand.get_chain_randomness_blocking(20, false).unwrap(),
            canonical,
            "repairing the lookup table must restore canonical randomness",
        );
    }

    #[test]
    fn repair_tipset_lookup_window_errors_on_missing_parent() {
        let db = Arc::new(MemoryDB::default());
        let genesis = genesis_tipset();
        persist_tipset(&genesis, &db);
        // Epoch 1 is deliberately not persisted: the walk from epoch 2 cannot verify
        // the window and must not report success.
        let epoch1 = tipset_child(&genesis, 1);
        let epoch2 = tipset_child(&epoch1, 2);
        persist_tipset(&epoch2, &db);

        let index = ChainIndex::new(db, genesis);
        assert!(index.repair_tipset_lookup_window(&epoch2, 900, 0).is_err());
    }

    #[test]
    fn repair_tipset_lookup_debounces_per_head_key() {
        let db = Arc::new(MemoryDB::default());
        let (genesis, partial, full, head) = chain_with_competing_tipsets_at_checkpoint(&db);
        let cs = ChainStore::new(db.clone(), Arc::new(ChainConfig::default()), genesis).unwrap();
        cs.set_heaviest_tipset(head.shallow_clone()).unwrap();

        // A clean scan records the head key.
        assert_eq!(cs.repair_tipset_lookup().unwrap(), 0);

        // Repeated failures at the same head are debounced: the poisoned entry is not
        // even looked at.
        db.set_tipset_key_at_epoch(&partial).unwrap();
        assert_eq!(cs.repair_tipset_lookup().unwrap(), 0);
        assert_eq!(
            db.tipset_key_by_epoch(20).unwrap().as_ref(),
            Some(partial.key())
        );

        // Any head change allows a rescan, which repairs the entry.
        let new_head = tipset_child(&head, 31);
        persist_tipset(&new_head, &db);
        cs.set_heaviest_tipset(new_head).unwrap();
        assert_eq!(cs.repair_tipset_lookup().unwrap(), 1);
        assert_eq!(
            db.tipset_key_by_epoch(20).unwrap().as_ref(),
            Some(full.key())
        );
    }

    #[test]
    fn update_tipset_lookup_for_finalized_head_deletes_null_round_entry() {
        let db = Arc::new(MemoryDB::default());
        let genesis = genesis_tipset();
        persist_tipset(&genesis, &db);
        // Epoch 20 is a null round: 19 is followed directly by 21.
        let mut prev = genesis.shallow_clone();
        for epoch in [10, 19, 21, 30] {
            let ts = tipset_child(&prev, epoch);
            persist_tipset(&ts, &db);
            prev = ts;
        }
        let head = prev;

        // A stale entry at the null checkpoint epoch, e.g. left behind by a reorg. Only
        // the table entry matters; the maintenance walk never loads the stale tipset.
        let stale = tipset_child(&genesis, 20);
        db.set_tipset_key_at_epoch(&stale).unwrap();

        let index = ChainIndex::new(db.clone(), genesis);
        index
            .update_tipset_lookup_for_finalized_head(&head, 25)
            .unwrap();
        assert_eq!(db.tipset_key_by_epoch(20).unwrap(), None);
    }

    #[test]
    fn tipset_by_height_broken_ancestor_chain_returns_none() {
        let db = Arc::new(MemoryDB::default());
        let genesis = genesis_tipset();
        // Epoch 3 header points at a parent key we never persist — `Tipset::chain` stops
        // after this tipset, so `tipset_by_height` finds no `(child, parent)` window.
        let epoch3 = tipset_child(&tipset_child(&genesis, 2), 3);
        persist_tipset(&genesis, &db);
        persist_tipset(&epoch3, &db);

        let index = ChainIndex::new(db, genesis);
        assert!(
            index
                .tipset_by_height_blocking(2, epoch3, ResolveNullTipset::TakeOlder)
                .unwrap()
                .is_none()
        );
    }
}
