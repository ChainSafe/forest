// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! This module implements a garbage collector that transforms parity-db into an effective
//! lite snapshot then purges parity-db.
//!
//! ## Design goals
//! A correct GC algorithm that is simple and efficient for forest scenarios. This algorithm
//! removes all blocks that are not included in an effective standard lite snapshot with
//! 2000 epochs of most recent state-trees and messages.
//!
//! ## GC Workflow
//! 1. Export an effective standard lite snapshot in `.forest.car.zst` format that can be used for
//!    bootstrapping a Filecoin node into the CAR database.
//! 3. Purge parity-db columns that serve as non-persistent blockstore.
//! 4. Purge old CAR database files.
//!
//! ## Correctness
//! The algorithm assumes that a Forest node can always be bootstrapped with the most recent standard lite snapshot.
//!
//! ### RAM/Disk Usage Spikes
//!
//! During the GC process, Forest consumes extra RAM and disk space temporarily:
//!
//! - While traversing reachable blocks, it uses ~80MiB of RAM and ~8GiB disk space on mainnet (and ~2GiB on calibnet) for de-duplicating reachable blocks.
//! - While exporting a lite snapshot, it uses extra disk space before cleaning up parity-db and stale CAR snapshots.
//!
//! For a typical ~80 GiB mainnet snapshot, this results in ~80 MiB of additional RAM and ~90 GiB disk space usage.
//!
//! ## Scheduling
//! When automatic GC is enabled, it by default runs every 7 days (20160 epochs).
//! The interval can be overridden by setting environment variable `FOREST_SNAPSHOT_GC_INTERVAL_EPOCHS`.
//!
//! ## Performance
//! The lite snapshot export step is currently utilizing a depth-first search algorithm, with `O(V+E)` complexity,
//! where V is the number of vertices(state-trees and messages) and E is the number of edges(block headers).
//!

use crate::blocks::{Tipset, TipsetKey};
use crate::chain::{ChainStore, ExportOptions};
use crate::chain_sync::ChainFollower;
use crate::cid_collections::FileBackedCidHashSet;
use crate::cli_shared::chain_path;
use crate::db::DbImpl;
use crate::db::{
    BlockstoreWriteOpsSubscribable, CAR_DB_DIR_NAME, EthBlockBloomStore, HeaviestTipsetKeyProvider,
    car::{ForestCar, ReloadableManyCar, forest::new_forest_car_temp_path_in},
    db_engine::db_root,
    parity_db::GarbageCollectableDb,
};
use crate::interpreter::VMTrace;
use crate::ipld::{ChainExportGuard, ChainExportKind};
use crate::prelude::*;
use crate::shim::clock::EPOCHS_IN_DAY;
use crate::utils::io::EitherMmapOrRandomAccessFile;
use ahash::HashMap;
use anyhow::Context as _;
use human_repr::HumanCount as _;
use parking_lot::RwLock;
use sha2::Sha256;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
use tokio::task::JoinSet;

pub struct SnapshotGarbageCollector {
    chain_tmp_root: PathBuf,
    car_db_dir: PathBuf,
    recent_state_roots: i64,
    running: AtomicBool,
    blessed_lite_snapshot: RwLock<Option<PathBuf>>,
    chain_follower: ChainFollower,
    // On mainnet, it takes ~50MiB-200MiB RAM, depending on the time cost of snapshot export
    memory_db: RwLock<Option<HashMap<Cid, bytes::Bytes>>>,
    memory_db_head_key: RwLock<Option<TipsetKey>>,
    exported_head_key: RwLock<Option<TipsetKey>>,
    trigger_tx: flume::Sender<Option<GcOutcomeSender>>,
    trigger_rx: flume::Receiver<Option<GcOutcomeSender>>,
}

/// Delivers the outcome of a GC run to a blocking `Forest.SnapshotGC` caller.
type GcOutcomeSender = flume::Sender<anyhow::Result<()>>;

impl SnapshotGarbageCollector {
    pub fn new(chain_follower: ChainFollower, config: &crate::Config) -> anyhow::Result<Self> {
        let chain_data_path = chain_path(config);
        let chain_tmp_root = chain_data_path.join("tmp");
        // Clear in case there're left-overs from the last node run.
        if chain_tmp_root.is_dir()
            && let Err(e) = std::fs::remove_dir_all(&chain_tmp_root)
        {
            tracing::warn!(
                "failed to clear tmp folder {}: {e}",
                chain_tmp_root.display()
            );
        }
        std::fs::create_dir_all(&chain_tmp_root)?;
        let db_root_dir = db_root(&chain_data_path)?;
        let car_db_dir = db_root_dir.join(CAR_DB_DIR_NAME);
        let recent_state_roots = std::env::var("FOREST_SNAPSHOT_GC_KEEP_STATE_TREE_EPOCHS")
            .ok()
            .and_then(|i| {
                i.parse().ok().and_then(|i| {
                    if i >= config.sync.recent_state_roots {
                        tracing::info!("Snapshot GC is set to keep {i} epochs of state trees");
                        Some(i)
                    } else {
                        tracing::warn!("Snapshot GC cannot be set to keep {i} epochs of state trees, at least {} is required for snapshot export", config.sync.recent_state_roots);
                        None
                    }
                })
            })
            .unwrap_or(config.sync.recent_state_roots);
        let (trigger_tx, trigger_rx) = flume::bounded(1);
        Ok(Self {
            chain_tmp_root,
            car_db_dir,
            recent_state_roots,
            running: AtomicBool::new(false),
            blessed_lite_snapshot: RwLock::new(None),
            chain_follower,
            memory_db: RwLock::new(None),
            memory_db_head_key: RwLock::new(None),
            exported_head_key: RwLock::new(None),
            trigger_tx,
            trigger_rx,
        })
    }

    pub async fn event_loop(&self) {
        while let Ok(outcome_tx) = self.trigger_rx.recv_async().await {
            self.gc_once(outcome_tx).await;
        }
    }

    pub async fn scheduler_loop(&self) {
        let snap_gc_interval_epochs = std::env::var("FOREST_SNAPSHOT_GC_INTERVAL_EPOCHS")
            .ok()
            .and_then(|i| i.parse().ok())
            .inspect(|i| {
                tracing::info!(
                    "Using snapshot GC interval epochs {i} set by FOREST_SNAPSHOT_GC_INTERVAL_EPOCHS"
                )
            })
            .unwrap_or(EPOCHS_IN_DAY * 7);
        let snap_gc_check_interval_secs = std::env::var("FOREST_SNAPSHOT_GC_CHECK_INTERVAL_SECONDS")
            .ok()
            .and_then(|i| i.parse().ok())
            .inspect(|i| {
                tracing::info!(
                    "Using snapshot GC check interval seconds {i} set by FOREST_SNAPSHOT_GC_CHECK_INTERVAL_SECONDS"
                )
            })
            .unwrap_or(60 * 5);
        let snap_gc_check_interval = Duration::from_secs(snap_gc_check_interval_secs);
        tracing::info!(
            "Running snapshot GC scheduler with interval epochs {snap_gc_interval_epochs}"
        );
        loop {
            if !self.running.load(Ordering::Relaxed)
                && let Some(car_db_head_epoch) =
                    self.db().heaviest_car_tipset().ok().map(|ts| ts.epoch())
            {
                let sync_status = (*self.sync_status().load()).shallow_clone();
                let network_head_epoch = sync_status.network_head_epoch;
                let head_epoch = sync_status.current_head_epoch;
                if head_epoch > 0 // sync_status has been initialized
                    && head_epoch <= network_head_epoch // head epoch is within a sane range
                    && sync_status.is_synced() // chain is in sync
                    && sync_status.active_forks.is_empty() // no active fork
                    && head_epoch - car_db_head_epoch >= snap_gc_interval_epochs // the gap between chain head and car_db head is above threshold
                    && self.trigger_tx.try_send(None).is_ok()
                {
                    tracing::info!(%car_db_head_epoch, %head_epoch, %network_head_epoch, %snap_gc_interval_epochs, "Snap GC scheduled");
                } else {
                    tracing::debug!(%car_db_head_epoch, %head_epoch, %network_head_epoch, %snap_gc_interval_epochs, "Snap GC not scheduled");
                }
            }
            tokio::time::sleep(snap_gc_check_interval).await;
        }
    }

    /// Whether a snapshot GC run is currently in progress. Index backfill checks this to avoid
    /// reading historical state/blocks while the GC is reclaiming graph columns.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    pub fn trigger(&self) -> anyhow::Result<flume::Receiver<anyhow::Result<()>>> {
        if self.running.load(Ordering::Relaxed) {
            anyhow::bail!("snap gc has already been running");
        }

        // Travels with the trigger, so another run cannot consume it.
        let (outcome_tx, outcome_rx) = flume::bounded(1);
        if self.trigger_tx.try_send(Some(outcome_tx)).is_err() {
            anyhow::bail!("snap gc has already been triggered");
        }
        Ok(outcome_rx)
    }

    async fn gc_once(&self, outcome_tx: Option<GcOutcomeSender>) {
        if self.running.swap(true, Ordering::Relaxed) {
            tracing::warn!("snap gc has already been running");
            return;
        }
        let result = match self.export_snapshot().await {
            Ok(()) => self.cleanup_after_snapshot_export().await,
            Err(e) => {
                // Unsubscribe on failure path
                self.db().unsubscribe_write_ops();
                Err(e)
            }
        };
        if let Err(e) = &result {
            tracing::error!("{e:#}");
        }
        // Dropping the sender also unblocks the caller.
        if let Some(outcome_tx) = outcome_tx {
            let _ = outcome_tx.send(result);
        }
        self.running.store(false, Ordering::Relaxed);
    }

    async fn export_snapshot(&self) -> anyhow::Result<()> {
        let chain_export_guard = ChainExportGuard::try_start_export(ChainExportKind::SnapshotGc)?;
        let result = self.export_snapshot_inner(&chain_export_guard).await;
        chain_export_guard.finish(result)
    }

    async fn export_snapshot_inner(
        &self,
        chain_export_guard: &ChainExportGuard,
    ) -> anyhow::Result<()> {
        let db = self.db();
        tracing::info!(
            "exporting lite snapshot with {} recent state roots",
            self.recent_state_roots
        );
        let temp_path = new_forest_car_temp_path_in(&self.car_db_dir)?;
        let file = tokio::fs::File::create(&temp_path).await?;
        let mut db_write_ops_rx = db.subscribe_write_ops()?;
        let mut joinset = JoinSet::new();
        joinset.spawn(async move {
            let mut map = HashMap::default();
            loop {
                match db_write_ops_rx.recv().await {
                    Ok(pairs) => {
                        map.extend(pairs);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::warn!(
                            "{skipped} write ops lagged, skip backfilling from memory db"
                        );
                        map.clear();
                        break;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
            map
        });
        let start = Instant::now();
        let head_ts = self.chain_follower.state_manager.heaviest_tipset();
        let state_compute_and_export = async {
            // The RPC cache prefilling logic might have run state computation for the same tipset before `db.subscribe_write_ops()`.
            // We run it again to ensure the output is tracked in the backfilling memory-db, because the output state tree is
            // not exported as part of the GC snapshot.
            self.chain_follower
                .state_manager
                .compute_tipset_state(
                    head_ts.shallow_clone(),
                    crate::state_manager::NO_CALLBACK,
                    VMTrace::NotTraced,
                )
                .await?;
            crate::chain::export::<Sha256, _>(
                db,
                &head_ts,
                self.recent_state_roots,
                file,
                ExportOptions {
                    skip_checksum: true,
                    include_receipts: true,
                    include_events: true,
                    include_tipset_keys: true,
                    include_tipset_lookup: false,
                    seen: FileBackedCidHashSet::new(&self.chain_tmp_root)?,
                },
            )
            .await
        };
        chain_export_guard
            .run_cancellable(state_compute_and_export)
            .await
            .context("snapshot GC export was cancelled")??;
        let target_path = self.car_db_dir.join(format!(
            "lite_{}_{}.forest.car.zst",
            self.recent_state_roots,
            head_ts.epoch()
        ));
        let target_path = crate::utils::spawn_blocking_with_timeout(
            crate::db::car::forest::ASYNC_OPS_TIMEOUT,
            move || {
                temp_path.persist(&target_path)?;
                Ok(target_path)
            },
        )
        .await
        .context("failed to persist the GC lite snapshot")?;
        tracing::info!(
            "exported lite snapshot at {}, took {}",
            target_path.display(),
            humantime::format_duration(start.elapsed())
        );
        *self.blessed_lite_snapshot.write() = Some(target_path);
        *self.exported_head_key.write() = Some(head_ts.key().clone());
        let current_chain_head = db.heaviest_tipset_key().ok().flatten();
        // Unsubscribe before taking the snapshot of in-memory db to avoid deadlock
        db.unsubscribe_write_ops();
        match joinset.join_next().await {
            Some(Ok(map)) if !map.is_empty() => {
                *self.memory_db.write() = Some(map);
                *self.memory_db_head_key.write() = current_chain_head;
            }
            Some(Err(e)) => tracing::warn!("{e}"),
            _ => {}
        }
        joinset.shutdown().await;
        Ok(())
    }

    async fn cleanup_after_snapshot_export(&self) -> anyhow::Result<()> {
        tracing::info!("cleaning up db");
        if let Some(blessed_lite_snapshot) = { self.blessed_lite_snapshot.read().clone() }
            && blessed_lite_snapshot.is_file()
            && ForestCar::is_valid(&EitherMmapOrRandomAccessFile::open(
                blessed_lite_snapshot.as_path(),
            )?)
        {
            let db = self.db();

            // Reset parity-db columns
            tokio::task::spawn_blocking({
                let db = db.shallow_clone();
                move || db.reset_gc_columns()
            })
            .await??;

            // Backfill new db records during snapshot export
            if let Some(mem_db) = self.memory_db.write().take() {
                let count = mem_db.len();
                let approximate_heap_size = {
                    let mut size = 0;
                    for v in mem_db.values() {
                        size += std::mem::size_of::<Cid>();
                        size += v.len();
                    }
                    size
                };
                let start = Instant::now();
                db.put_many_keyed(mem_db)?;
                tracing::info!(
                    "backfilled {count} new db records since snapshot epoch, approximate heap size: {}, took {}",
                    approximate_heap_size.human_count_bytes(),
                    humantime::format_duration(start.elapsed())
                );
            }

            // Reload CAR files
            db.clear_and_reload_cars(std::iter::once(blessed_lite_snapshot.clone()))?;
            tracing::info!(
                "reloaded car db at {} with head epoch {}",
                blessed_lite_snapshot.display(),
                db.heaviest_car_tipset()
                    .map(|ts| ts.epoch())
                    .unwrap_or_default()
            );

            // Prune blooms whose events are no longer retained by the lite snapshot.
            if let Ok(head) = db.heaviest_car_tipset() {
                let cutoff = head.epoch() - self.recent_state_roots;
                if let Err(e) = db.delete_blooms_before_height(cutoff) {
                    tracing::warn!("failed to prune stale block blooms: {e:#}");
                }
            }

            // Reset chain head. Note that `self.exported_head_key` is guaranteed to be present,
            // see `*self.exported_head_key.write() = Some(head_ts.key().clone());` in `export_snapshot`.
            for tsk_opt in [
                self.memory_db_head_key.write().take(),
                self.exported_head_key.write().take(),
            ] {
                if let Some(tsk) = tsk_opt
                    && let Ok(ts) = Tipset::load_required(&db, &tsk)
                {
                    let epoch = ts.epoch();
                    if let Err(e) = self.cs().set_heaviest_tipset(ts) {
                        tracing::error!(
                            "failed to set chain head to epoch {epoch} with key {tsk}: {e:#}"
                        );
                    } else {
                        tracing::info!("set chain head to epoch {epoch} with key {tsk}");
                        break;
                    }
                }
            }

            // Reset chain follower
            self.chain_follower.reset();

            // Cleanup stale CAR files
            for car_to_remove in walkdir::WalkDir::new(&self.car_db_dir)
                .max_depth(1)
                .into_iter()
                .filter_map(|entry| {
                    if let Ok(entry) = entry {
                        // Also cleanup `.tmp*` files snapshot export is interrupted ungracefully
                        if entry.path().is_file() && entry.path() != blessed_lite_snapshot.as_path()
                        {
                            return Some(entry.into_path());
                        }
                    }
                    None
                })
            {
                match std::fs::remove_file(&car_to_remove) {
                    Ok(_) => {
                        tracing::info!("deleted car db at {}", car_to_remove.display());
                    }
                    Err(e) => {
                        tracing::warn!(
                            "failed to delete car db at {}: {e}",
                            car_to_remove.display()
                        );
                    }
                }
            }
        }
        Ok(())
    }

    fn cs(&self) -> &ChainStore {
        self.chain_follower.state_manager.chain_store()
    }

    fn db(&self) -> &DbImpl {
        self.chain_follower.state_manager.db()
    }

    fn sync_status(&self) -> &crate::chain_sync::SyncStatus {
        &self.chain_follower.sync_status
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::{CachingBlockHeader, RawBlockHeader};
    use crate::chain_sync::network_context::SyncNetworkContext;
    use crate::db::MemoryDB;
    use crate::ipld::{ChainExportGuard, ChainExportKind};
    use crate::libp2p::PeerManager;
    use crate::message_pool::MessagePool;
    use crate::networks::ChainConfig;
    use crate::shim::address::Address;
    use crate::state_manager::StateManager;
    use tokio::task::JoinSet;

    fn test_gc(
        data_dir: &std::path::Path,
    ) -> (SnapshotGarbageCollector, JoinSet<anyhow::Result<()>>) {
        let (network_send, _network_rx) = flume::bounded(5);
        let (_net_event_tx, net_event_rx) = flume::bounded(5);
        let mut services = JoinSet::new();
        let db = std::sync::Arc::new(MemoryDB::default());
        let chain_config = std::sync::Arc::new(ChainConfig::default());
        let genesis_header = CachingBlockHeader::new(RawBlockHeader {
            miner_address: Address::new_id(0),
            timestamp: 7777,
            ..Default::default()
        });
        let cs = ChainStore::new(db, chain_config, genesis_header.clone()).unwrap();
        let state_manager = StateManager::new(cs.shallow_clone()).unwrap();
        let mpool = MessagePool::new(
            cs,
            network_send.clone(),
            Default::default(),
            state_manager.chain_config().clone(),
            &mut services,
        )
        .unwrap();
        let genesis_ts = Tipset::from(genesis_header);
        let peer_manager = std::sync::Arc::new(PeerManager::default());
        let network = SyncNetworkContext::new(network_send, peer_manager, state_manager.db_owned());
        let chain_follower = ChainFollower::new(
            state_manager,
            network,
            genesis_ts,
            net_event_rx,
            false,
            mpool,
        );
        let mut config = crate::Config::default();
        config.client.data_dir = data_dir.to_path_buf();
        let gc = SnapshotGarbageCollector::new(chain_follower, &config).unwrap();
        (gc, services)
    }

    /// `forest-cli chain prune snap` must not exit 0 with the GC error only in the
    /// daemon logs.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[serial_test::serial(chain_export)]
    async fn manual_gc_trigger_propagates_failure() {
        let tmp = tempfile::TempDir::new().unwrap();
        let (gc, _services) = test_gc(tmp.path());
        let gc = std::sync::Arc::new(gc);
        tokio::spawn({
            let gc = gc.clone();
            async move { gc.event_loop().await }
        });

        // Hold the export slot so the GC export cannot start.
        let _guard = ChainExportGuard::try_start_export(ChainExportKind::Snapshot).unwrap();

        let progress_rx = gc.trigger().unwrap();
        let outcome = tokio::time::timeout(Duration::from_secs(10), progress_rx.recv_async())
            .await
            .expect("GC must complete")
            .expect("a blocking GC caller must receive the outcome");
        assert!(
            outcome.is_err(),
            "GC that could not start must report failure to the caller"
        );
    }
}
