// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//!
//! The current implementation of the garbage collector is a concurrent,
//! semi-space one.
//!
//! ## Design goals
//! Implement a correct GC algorithm that is simple and efficient for forest
//! scenarios.
//!
//! ## GC algorithm
//! We chose the `semi-space` GC algorithm for simplicity and sufficiency
//! Besides `semi-space`, `mark-and-sweep` was also considered and evaluated.
//! However, it's not feasible because of the limitations of the underlying DB
//! we use, more specifically, limitations in iterating the DB and retrieving the original key. See <https://github.com/paritytech/parity-db/issues/187>
//!
//! ## GC workflow
//! 1. Walk back from the current heaviest tipset to the genesis block, collect
//! all the blocks that are reachable from the snapshot
//! 2. writes blocks that are absent from the `current` database to it
//! 3. delete `old` database(s)
//! 4. sets `current` database to a newly created one
//!
//! ## Correctness
//! This algorithm considers all blocks that are visited during the snapshot
//! export task reachable, and ensures they are all transferred and kept in the
//! current DB space. A snapshot can be used to bootstrap a node from
//! scratch thus the algorithm is considered appropriate when the post-GC
//! database contains blocks that are sufficient for exporting a snapshot
//!
//! ## Disk usage
//! During `walk_snapshot`, data from the `old` DB is duplicated in the
//! `current` DB, which uses extra disk space of up to 100% of the snapshot file
//! size
//!
//! ## Memory usage
//! During the data carry-over process, a memory buffer with a fixed capacity is
//! used to speed up the database write operation
//!
//! ## Scheduling
//! 1. GC is triggered automatically when total DB size is greater than `2x` of
//! the last reachable data size
//! 2. GC can be triggered manually by `forest-cli db gc` command
//! 3. There's a global GC lock to ensure at most one GC job is running
//!
//! ## Performance
//! GC performance is typically `1x-1.5x` of `snapshot export`, depending on
//! number of write operations to the `current` DB space.
//!
//! ### Look up performance
//! DB lookup performance is almost on-par between from single DB and two DBs.
//! Time cost of `forest-cli snapshot export --dry-run` on DO droplet with 16
//! GiB RAM is between `9000s` to `11000s` for both scenarios, no significant
//! performance regression has been observed
//!
//! ### Write performance
//! DB write performance is typically on par with `snapshot import`. Note that
//! when the `current` DB space is very large, it tends to trigger DB re-index
//! more frequently, each DB re-index could pause the GC process for a few
//! minutes. The same behavior is observed during snapshot import as well.
//!
//! ### Sample mainnet log
//! ```text
//! 2023-03-16T19:50:40.323860Z  INFO forest_db::rolling::gc: Garbage collection started at epoch 2689660
//! 2023-03-16T22:27:36.484245Z  INFO forest_db::rolling::gc: Garbage collection finished at epoch 2689660, took 9416s, reachable data size: 135.71GB
//! 2023-03-16T22:27:38.793717Z  INFO forest_db::rolling::impls: Deleted database under /root/.local/share/forest/mainnet/paritydb/14d0f80992374fb8b20e3b1bd70d5d7b, size: 139.01GB
//! ```

use std::{
    sync::atomic::{self, AtomicU64, AtomicUsize},
    time::Duration,
};

use chrono::Utc;
use forest_blocks::Tipset;
use forest_ipld::util::*;
use forest_utils::db::{BlockstoreBufferedWriteExt, DB_KEY_BYTES};
use fvm_ipld_blockstore::Blockstore;
use human_repr::HumanCount;
use tokio::sync::Mutex;

use super::*;

pub struct DbGarbageCollector<F>
where
    F: Fn() -> Tipset + Send + Sync + 'static,
{
    db: RollingDB,
    get_tipset: F,
    chain_finality: i64,
    lock: Mutex<()>,
    gc_tx: flume::Sender<flume::Sender<anyhow::Result<()>>>,
    gc_rx: flume::Receiver<flume::Sender<anyhow::Result<()>>>,
    last_reachable_bytes: AtomicU64,
}

impl<F> DbGarbageCollector<F>
where
    F: Fn() -> Tipset + Send + Sync + 'static,
{
    pub fn new(db: RollingDB, chain_finality: i64, get_tipset: F) -> Self {
        let (gc_tx, gc_rx) = flume::unbounded();

        Self {
            db,
            get_tipset,
            chain_finality,
            lock: Default::default(),
            gc_tx,
            gc_rx,
            last_reachable_bytes: AtomicU64::new(0),
        }
    }

    pub fn get_tx(&self) -> flume::Sender<flume::Sender<anyhow::Result<()>>> {
        self.gc_tx.clone()
    }

    /// This loop automatically triggers `collect_once` when the total DB size
    /// is greater than `2x` of the last reachable data size
    pub async fn collect_loop_passive(&self) -> anyhow::Result<()> {
        info!("Running automatic database garbage collection task");
        loop {
            // Check every 10 mins
            tokio::time::sleep(Duration::from_secs(10 * 60)).await;

            // Bypass size checking during import
            let tipset = (self.get_tipset)();
            if tipset.epoch() == 0 {
                continue;
            }

            // Bypass size checking when lock is held
            {
                let lock = self.lock.try_lock();
                if lock.is_err() {
                    continue;
                }
            }

            if let (Ok(total_size), Ok(current_size), last_reachable_bytes) = (
                self.db.total_size_in_bytes(),
                self.db.current_size_in_bytes(),
                self.last_reachable_bytes.load(atomic::Ordering::Relaxed),
            ) {
                let should_collect = if last_reachable_bytes > 0 {
                    total_size > (gc_trigger_factor() * last_reachable_bytes as f64) as _
                } else {
                    total_size > 0 && current_size * 3 > total_size
                };

                if should_collect {
                    if let Err(err) = self.collect_once().await {
                        warn!("Garbage collection failed: {err}");
                    }
                }
            }
        }
    }

    /// This loop listens on events emitted by `forest-cli db gc` and triggers
    /// `collect_once`
    pub async fn collect_loop_event(self: &Arc<Self>) -> anyhow::Result<()> {
        info!("Listening on database garbage collection events");
        while let Ok(responder) = self.gc_rx.recv_async().await {
            let this = self.clone();
            tokio::spawn(async move {
                let result = this.collect_once().await;
                if let Err(e) = responder.send(result) {
                    warn!("{e}");
                }
            });
        }

        Ok(())
    }

    /// ## GC workflow
    /// 1. Walk back from the current heaviest tipset to the genesis block,
    /// collect all the blocks that are reachable from the snapshot
    /// 2. writes blocks that are absent from the `current` database to it
    /// 3. delete `old` database(s)
    /// 4. sets `current` database to a newly created one
    ///
    /// ## Data Safety
    /// The blockchain consists of an immutable part (tipsets that are at least
    /// 900 epochs older than the current head) and a mutable part (tipsets
    /// that are within the most recent 900 epochs). Deleting data from the
    /// mutable part of the chain can be problematic; therefore, we record the
    /// exact epoch at which a new current database space is created, and only
    /// perform garbage collection when this creation epoch has become
    /// immutable (at least 900 epochs older than the current head), thus
    /// the old database space that will be deleted at the end of garbage
    /// collection only contains immutable or finalized part of the chain,
    /// from which all block data that is marked as unreachable will not
    /// become reachable because of the chain being mutated later.
    async fn collect_once(&self) -> anyhow::Result<()> {
        let tipset = (self.get_tipset)();

        if self.db.current_creation_epoch() + self.chain_finality >= tipset.epoch() {
            anyhow::bail!("Cancelling GC: the old DB space contains unfinalized chain parts");
        }

        let guard = self.lock.try_lock();
        if guard.is_err() {
            anyhow::bail!("Another garbage collection task is in progress.");
        }

        let start = Utc::now();
        let reachable_bytes = Arc::new(AtomicUsize::new(0));

        info!("Garbage collection started at epoch {}", tipset.epoch());
        let db = &self.db;
        // 128MB
        const BUFFER_CAPCITY_BYTES: usize = 128 * 1024 * 1024;
        let (tx, rx) = flume::bounded(100);
        #[allow(clippy::redundant_async_block)]
        let write_task = tokio::spawn({
            let db = db.current();
            async move { db.buffered_write(rx, BUFFER_CAPCITY_BYTES).await }
        });
        walk_snapshot(&tipset, DEFAULT_RECENT_STATE_ROOTS, |cid| {
            let db = db.clone();
            let tx = tx.clone();
            let reachable_bytes = reachable_bytes.clone();
            async move {
                let block = db
                    .get(&cid)?
                    .ok_or_else(|| anyhow::anyhow!("Cid {cid} not found in blockstore"))?;

                let pair = (cid, block.clone());
                reachable_bytes.fetch_add(DB_KEY_BYTES + pair.1.len(), atomic::Ordering::Relaxed);
                if !db.current().has(&cid)? {
                    tx.send_async(pair).await?;
                }

                Ok(block)
            }
        })
        .await?;
        drop(tx);
        write_task.await??;

        let reachable_bytes = reachable_bytes.load(atomic::Ordering::Relaxed);
        self.last_reachable_bytes
            .store(reachable_bytes as _, atomic::Ordering::Relaxed);
        info!(
            "Garbage collection finished at epoch {}, took {}s, reachable data size: {}",
            tipset.epoch(),
            (Utc::now() - start).num_seconds(),
            reachable_bytes.human_count_bytes(),
        );

        // Use the latest head here
        self.db.next_current((self.get_tipset)().epoch())?;

        Ok(())
    }
}

fn gc_trigger_factor() -> f64 {
    const DEFAULT_GC_TRIGGER_FACTOR: f64 = 2.0;

    if let Ok(factor) = std::env::var("FOREST_GC_TRIGGER_FACTOR") {
        factor.parse().unwrap_or(DEFAULT_GC_TRIGGER_FACTOR)
    } else {
        DEFAULT_GC_TRIGGER_FACTOR
    }
}
