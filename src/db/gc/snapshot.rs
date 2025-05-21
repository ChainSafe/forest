// Copyright 2019-2025 ChainSafe Systems
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
//! 1. Exports an effective standard lite snapshot in `.forest.car.zst` format that can be used for
//!    bootstrapping a Filecoin node into the CAR database.
//! 2. Stop the node.
//! 3. Purging parity-db columns that serve as non-persistent blockstore.
//! 4. Purging old CAR database files.
//! 5. Restart the node.
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
//! To be implemented
//!
//! ## Performance
//! The lite snapshot export step is currently utilizing a depth-first search algorithm, with `O(V+E)` complexity,
//! where V is the number of vertices(state-trees and messages) and E is the number of edges(block headers).
//!
//! ## Trade-offs
//! - All TCP interfaces are rebooted, thus all operations that interact with the TCP interfaces(e.g. `forest-cli sync wait`)
//!   are interrupted.
//!

use crate::blocks::{Tipset, TipsetKey};
use crate::cid_collections::CidHashSet;
use crate::cli_shared::chain_path;
use crate::db::db_engine::open_db;
use crate::db::{BlockstoreWriteOpsSubscribable, HeaviestTipsetKeyProvider};
use crate::db::{
    CAR_DB_DIR_NAME, SettingsStore,
    car::forest::FOREST_CAR_FILE_EXTENSION,
    db_engine::{DbConfig, db_root},
    parity_db::{DbColumn, ParityDb},
};
use ahash::HashMap;
use anyhow::Context as _;
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
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
    db_root_dir: PathBuf,
    car_db_dir: PathBuf,
    recent_state_roots: i64,
    db_config: DbConfig,
    running: AtomicBool,
    blessed_lite_snapshot: RwLock<Option<PathBuf>>,
    db: RwLock<Option<Arc<DB>>>,
    // On mainnet, it takes ~50MiB-200MiB RAM, depending on the time cost of snapshot export
    memory_db: RwLock<Option<HashMap<Cid, Vec<u8>>>>,
    memory_db_head_key: RwLock<Option<TipsetKey>>,
    exported_head_key: RwLock<Option<TipsetKey>>,
    reboot_tx: flume::Sender<()>,
    trigger_tx: flume::Sender<()>,
    trigger_rx: flume::Receiver<()>,
    progress_tx: RwLock<Option<flume::Sender<()>>>,
}

impl<DB> SnapshotGarbageCollector<DB>
where
    DB: Blockstore
        + SettingsStore
        + HeaviestTipsetKeyProvider
        + BlockstoreWriteOpsSubscribable
        + Send
        + Sync
        + 'static,
{
    pub fn new(config: &crate::Config) -> anyhow::Result<(Self, flume::Receiver<()>)> {
        let chain_data_path = chain_path(config);
        let db_root_dir = db_root(&chain_data_path)?;
        let car_db_dir = db_root_dir.join(CAR_DB_DIR_NAME);
        let recent_state_roots = config.sync.recent_state_roots;
        let (reboot_tx, reboot_rx) = flume::bounded(1);
        let (trigger_tx, trigger_rx) = flume::bounded(1);
        Ok((
            Self {
                db_root_dir,
                car_db_dir,
                recent_state_roots,
                db_config: config.db_config().clone(),
                running: AtomicBool::new(false),
                blessed_lite_snapshot: RwLock::new(None),
                db: RwLock::new(None),
                memory_db: RwLock::new(None),
                memory_db_head_key: RwLock::new(None),
                exported_head_key: RwLock::new(None),
                reboot_tx,
                trigger_tx,
                trigger_rx,
                progress_tx: RwLock::new(None),
            },
            reboot_rx,
        ))
    }

    pub fn set_db(&self, db: Arc<DB>) {
        *self.db.write() = Some(db);
    }

    pub async fn event_toop(&self) {
        while self.trigger_rx.recv_async().await.is_ok() {
            if self.running.load(Ordering::Relaxed) {
                tracing::warn!("snap gc has already been running");
            } else {
                self.running.store(true, Ordering::Relaxed);
                if let Err(e) = self.export_snapshot().await {
                    tracing::warn!("{e}");
                }
            }
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

    async fn export_snapshot(&self) -> anyhow::Result<()> {
        let db = self.db.write().take().context("db not yet initialzied")?;
        tracing::info!(
            "exporting lite snapshot with {} recent state roots",
            self.recent_state_roots
        );
        let temp_path = tempfile::NamedTempFile::new_in(&self.car_db_dir)?.into_temp_path();
        let file = tokio::fs::File::create(&temp_path).await?;
        let mut rx = db.subscribe_write_ops();
        let (cancel_tx, cancel_rx) = flume::bounded(1);
        let mut joinset = JoinSet::new();
        joinset.spawn(async move {
            let mut map = HashMap::default();
            loop {
                tokio::select! {
                    _ = cancel_rx.recv_async() => {break}
                    pair = rx.recv() => match pair {
                        Ok((k,v)) => {
                            map.insert(k, v);
                        }
                        _ => {break}
                    }
                }
            }
            map
        });
        let (head_ts, _) = crate::chain::export_from_head::<Sha256>(
            &db,
            self.recent_state_roots,
            file,
            CidHashSet::default(),
            true,
        )
        .await?;
        let target_path = self.car_db_dir.join(format!(
            "lite_{}_{}.forest.car.zst",
            self.recent_state_roots,
            head_ts.epoch()
        ));
        temp_path.persist(&target_path)?;
        tracing::info!("exported lite snapshot at {}", target_path.display());
        *self.blessed_lite_snapshot.write() = Some(target_path);
        *self.exported_head_key.write() = Some(head_ts.key().clone());

        if let Err(e) = self.reboot_tx.send(()) {
            tracing::warn!("{e}");
        }

        *self.memory_db_head_key.write() = db.heaviest_tipset_key().ok();
        tokio::time::sleep(Duration::from_secs(1)).await;
        let _ = cancel_tx.send(());
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

    pub async fn cleanup_before_reboot(&self) {
        drop(self.progress_tx.write().take());
        if let Err(e) = self.cleanup_before_reboot_inner().await {
            tracing::warn!("{e}");
        }
        self.running.store(false, Ordering::Relaxed);
    }

    async fn cleanup_before_reboot_inner(&self) -> anyhow::Result<()> {
        tracing::info!("cleaning up db before rebooting");
        if let Some(blessed_lite_snapshot) = { self.blessed_lite_snapshot.read().clone() } {
            if blessed_lite_snapshot.is_file() {
                let mut opts = ParityDb::to_options(self.db_root_dir.clone(), &self.db_config);
                for col in [
                    DbColumn::GraphDagCborBlake2b256 as u8,
                    DbColumn::GraphFull as u8,
                ] {
                    let start = Instant::now();
                    tracing::info!("pruning parity-db column {col}...");
                    loop {
                        match parity_db::Db::reset_column(&mut opts, col, None) {
                            Ok(_) => break,
                            Err(_) => {
                                tokio::time::sleep(Duration::from_secs(1)).await;
                            }
                        }
                    }
                    tracing::info!(
                        "pruned parity-db column {col}, took {}",
                        humantime::format_duration(start.elapsed())
                    );
                }

                for car_to_remove in walkdir::WalkDir::new(&self.car_db_dir)
                    .max_depth(1)
                    .into_iter()
                    .filter_map(|entry| {
                        if let Ok(entry) = entry {
                            if entry.path() != blessed_lite_snapshot.as_path() {
                                if let Some(filename) = entry.file_name().to_str() {
                                    if filename.ends_with(FOREST_CAR_FILE_EXTENSION) {
                                        return Some(entry.into_path());
                                    }
                                }
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

                // Backfill new db records during snapshot export
                if let Ok(db) = open_db(self.db_root_dir.clone(), &self.db_config) {
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
                        if let Err(e) = db.put_many_keyed(mem_db) {
                            tracing::warn!("{e}");
                        }
                        tracing::info!(
                            "backfilled {count} new db records since snapshot epoch, approximate heap size: {}, took {}",
                            human_bytes::human_bytes(approximate_heap_size as f64),
                            humantime::format_duration(start.elapsed())
                        );
                    }
                    match (
                        self.memory_db_head_key.write().take(),
                        self.exported_head_key.write().take(),
                    ) {
                        (Some(head_key), _) if Tipset::load_required(&db, &head_key).is_ok() => {
                            let _ = db.set_heaviest_tipset_key(&head_key);
                            tracing::info!("set memory db head key");
                        }
                        (_, Some(head_key)) => {
                            let _ = db.set_heaviest_tipset_key(&head_key);
                            tracing::info!("set exported head key");
                        }
                        _ => {}
                    }
                }
            }
        }
        Ok(())
    }
}
