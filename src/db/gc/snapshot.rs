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
//! ## Disk usage
//! The algorithm requires extra disk space of the size of a most recent standard lite
//! snapshot(`~72 GiB` as of writing at epoch 4937270 on mainnet).
//!
//! ## Memory usage
//! During the lite snapshot export stage, the algorithm at least `32 bytes` of memory for each reachable block
//! while traversing the reachable graph. For a typical mainnet snapshot of about 100 GiB that adds up to
//! roughly 2.5 GiB.
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
use crate::cli_shared::chain_path;
use crate::db::car::{ForestCar, ReloadableManyCar, forest::new_forest_car_temp_path_in};
use crate::db::parity_db::GarbageCollectableDb;
use crate::db::{
    BlockstoreWriteOpsSubscribable, CAR_DB_DIR_NAME, HeaviestTipsetKeyProvider, SettingsStore,
    db_engine::db_root,
};
use crate::shim::clock::EPOCHS_IN_DAY;
use crate::utils::io::EitherMmapOrRandomAccessFile;
use ahash::HashMap;
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use human_repr::HumanCount as _;
use parking_lot::RwLock;
use sha2::Sha256;
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};
use tokio::task::JoinSet;

pub struct SnapshotGarbageCollector<DB> {
    car_db_dir: PathBuf,
    recent_state_roots: i64,
    running: AtomicBool,
    blessed_lite_snapshot: RwLock<Option<PathBuf>>,
    cs: Arc<ChainStore<DB>>,
    sync_status: crate::chain_sync::SyncStatus,
    // On mainnet, it takes ~50MiB-200MiB RAM, depending on the time cost of snapshot export
    memory_db: RwLock<Option<HashMap<Cid, Vec<u8>>>>,
    memory_db_head_key: RwLock<Option<TipsetKey>>,
    exported_head_key: RwLock<Option<TipsetKey>>,
    trigger_tx: flume::Sender<()>,
    trigger_rx: flume::Receiver<()>,
    progress_tx: RwLock<Option<flume::Sender<()>>>,
}

impl<DB> SnapshotGarbageCollector<DB>
where
    DB: Blockstore
        + GarbageCollectableDb
        + ReloadableManyCar
        + SettingsStore
        + HeaviestTipsetKeyProvider
        + BlockstoreWriteOpsSubscribable
        + Send
        + Sync
        + 'static,
{
    pub fn new(
        cs: Arc<ChainStore<DB>>,
        sync_status: crate::chain_sync::SyncStatus,
        config: &crate::Config,
    ) -> anyhow::Result<Self> {
        let chain_data_path = chain_path(config);
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
            car_db_dir,
            recent_state_roots,
            running: AtomicBool::new(false),
            blessed_lite_snapshot: RwLock::new(None),
            cs,
            sync_status,
            memory_db: RwLock::new(None),
            memory_db_head_key: RwLock::new(None),
            exported_head_key: RwLock::new(None),
            trigger_tx,
            trigger_rx,
            progress_tx: RwLock::new(None),
        })
    }

    pub async fn event_loop(&self) {
        while self.trigger_rx.recv_async().await.is_ok() {
            self.gc_once().await;
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
                && let Some(car_db_head_epoch) = self
                    .cs
                    .blockstore()
                    .heaviest_car_tipset()
                    .ok()
                    .map(|ts| ts.epoch())
            {
                let sync_status = &*self.sync_status.read();
                let network_head_epoch = sync_status.network_head_epoch;
                let head_epoch = sync_status.current_head_epoch;
                if head_epoch > 0 // sync_status has been initialized
                    && head_epoch <= network_head_epoch // head epoch is within a sane range
                    && sync_status.is_synced() // chain is in sync
                    && sync_status.active_forks.is_empty() // no active fork
                    && head_epoch - car_db_head_epoch >= snap_gc_interval_epochs // the gap between chain head and car_db head is above threshold
                    && self.trigger_tx.try_send(()).is_ok()
                {
                    tracing::info!(%car_db_head_epoch, %head_epoch, %network_head_epoch, %snap_gc_interval_epochs, "Snap GC scheduled");
                } else {
                    tracing::debug!(%car_db_head_epoch, %head_epoch, %network_head_epoch, %snap_gc_interval_epochs, "Snap GC not scheduled");
                }
            }
            tokio::time::sleep(snap_gc_check_interval).await;
        }
    }

    pub fn trigger(&self) -> anyhow::Result<flume::Receiver<()>> {
        if self.running.load(Ordering::Relaxed) {
            anyhow::bail!("snap gc has already been running");
        }

        if self.trigger_tx.try_send(()).is_err() {
            anyhow::bail!("snap gc has already been triggered");
        }

        let (progress_tx, progress_rx) = flume::unbounded();
        *self.progress_tx.write() = Some(progress_tx);
        Ok(progress_rx)
    }

    async fn gc_once(&self) {
        if self.running.load(Ordering::Relaxed) {
            tracing::warn!("snap gc has already been running");
        } else {
            self.running.store(true, Ordering::Relaxed);
            match self.export_snapshot().await {
                Ok(_) => {
                    if let Err(e) = self.cleanup_after_snapshot_export().await {
                        tracing::warn!("{e:#}");
                    }
                }
                Err(e) => {
                    tracing::error!("{e:#}");
                    // Unsubscribe on failure path
                    self.cs.blockstore().unsubscribe_write_ops();
                }
            }
            // To indicate the completion of GC
            drop(self.progress_tx.write().take());
            self.running.store(false, Ordering::Relaxed);
        }
    }

    async fn export_snapshot(&self) -> anyhow::Result<()> {
        let cs = &self.cs;
        let db = cs.blockstore();
        tracing::info!(
            "exporting lite snapshot with {} recent state roots",
            self.recent_state_roots
        );
        let mut db_write_ops_rx = db.subscribe_write_ops();
        let temp_path = new_forest_car_temp_path_in(&self.car_db_dir)?;
        let file = tokio::fs::File::create(&temp_path).await?;
        let mut joinset = JoinSet::new();
        joinset.spawn(async move {
            let mut map = HashMap::default();
            while let Ok((k, v)) = db_write_ops_rx.recv().await {
                map.insert(k, v);
            }
            map
        });
        let start = Instant::now();
        let (head_ts, _) = crate::chain::export_from_head::<Sha256>(
            db,
            self.recent_state_roots,
            file,
            Some(ExportOptions {
                skip_checksum: true,
                include_receipts: true,
                include_events: true,
                include_tipset_keys: true,
                seen: Default::default(),
            }),
        )
        .await?;
        let target_path = self.car_db_dir.join(format!(
            "lite_{}_{}.forest.car.zst",
            self.recent_state_roots,
            head_ts.epoch()
        ));
        temp_path.persist(&target_path)?;
        tracing::info!(
            "exported lite snapshot at {}, took {}",
            target_path.display(),
            humantime::format_duration(start.elapsed())
        );
        *self.blessed_lite_snapshot.write() = Some(target_path);
        *self.exported_head_key.write() = Some(head_ts.key().clone());
        *self.memory_db_head_key.write() = db.heaviest_tipset_key().ok().flatten();
        // Unsubscribe before taking the snapshot of in-memory db to avoid deadlock
        db.unsubscribe_write_ops();
        match joinset.join_next().await {
            Some(Ok(map)) => {
                *self.memory_db.write() = Some(map);
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
            let db = self.cs.blockstore();

            // Reset parity-db columns
            db.reset_gc_columns()?;

            // Backfill new db records during snapshot export
            if let Some(mem_db) = self.memory_db.write().take() {
                let count = mem_db.len();
                let approximate_heap_size = {
                    let mut size = 0;
                    for (_k, v) in mem_db.iter() {
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
                    if let Err(e) = self.cs.set_heaviest_tipset(ts) {
                        tracing::error!(
                            "failed to set chain head to epoch {epoch} with key {tsk}: {e:#}"
                        );
                    } else {
                        tracing::info!("set chain head to epoch {epoch} with key {tsk}");
                        break;
                    }
                }
            }

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
}
