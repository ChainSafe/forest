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
//! We choose `semi-space` GC algorithm for simplicity and sufficiency
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
//! `current` DB, which uses extra disk space of up-to 100% of the snapshot file
//! size
//!
//! ## Memory usage
//! During the data carry-over process, a memory buffer with a fixed capacity is
//! used to speed up the database write operation
//!
//! ## Scheduling
//! 1. GC is triggered automatically when total DB size is greater than 2x of
//! the last reachable data size
//! 2. GC can be triggered manually by `forest-cli db gc` command
//! 3. There's global GC lock to ensure at most one GC job is running
//!
//! ## Performance
//! GC performance is typically 1x-1.5x of `snapshot export`, depending on
//! number of write operations to the `current` DB space.
//!
//! ### Look up performance
//! DB lookup performance is almost on-par between from single DB and two DBs.
//! Time cost of `forest-cli snapshot export --dry-run` on DO droplet with 16GiB
//! ram is between `9000s` to `11000s` for both scenarios, no significant
//! performance regression has been observed
//!
//! ### Write performance
//! DB write performance is typically on-par with `snapshot import`, note that
//! when the `current` DB space is very large, it tends to trigger DB re-index
//! more frequently, each DB re-index could pause the GC process for a few
//! minutes. The same behaviour is observed during snapshot import as well.

use std::{
    sync::atomic::{self, AtomicU64, AtomicUsize},
    time::Duration,
};

use chrono::Utc;
use forest_blocks::Tipset;
use forest_ipld::util::*;
use fvm_ipld_blockstore::Blockstore;
use human_repr::HumanCount;
use tokio::sync::Mutex;

use super::*;
use crate::StoreExt;

/// 100GiB
const ESTIMATED_LAST_REACHABLE_BYTES_FOR_COLD_START: u64 = 100 * 1024_u64.pow(3);

pub struct DbGarbageCollector<F>
where
    F: Fn() -> Tipset + Send + Sync + 'static,
{
    db: RollingDB,
    get_tipset: F,
    lock: Mutex<()>,
    gc_tx: flume::Sender<flume::Sender<anyhow::Result<()>>>,
    gc_rx: flume::Receiver<flume::Sender<anyhow::Result<()>>>,
    last_reachable_bytes: AtomicU64,
}

impl<F> DbGarbageCollector<F>
where
    F: Fn() -> Tipset + Send + Sync + 'static,
{
    pub fn new(db: RollingDB, get_tipset: F) -> Self {
        let (gc_tx, gc_rx) = flume::unbounded();

        Self {
            db,
            get_tipset,
            lock: Default::default(),
            gc_tx,
            gc_rx,
            last_reachable_bytes: AtomicU64::new(ESTIMATED_LAST_REACHABLE_BYTES_FOR_COLD_START),
        }
    }

    pub fn get_tx(&self) -> flume::Sender<flume::Sender<anyhow::Result<()>>> {
        self.gc_tx.clone()
    }

    /// This loop automatically triggers `collect_once` when the total DB size
    /// is greater than 2x of the last reachable data size
    pub async fn collect_loop_passive(&self) -> anyhow::Result<()> {
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

            if let (Ok(total_size), mut last_reachable_bytes) = (
                self.db.total_size_in_bytes(),
                self.last_reachable_bytes.load(atomic::Ordering::Relaxed),
            ) {
                if last_reachable_bytes == 0 {
                    last_reachable_bytes = ESTIMATED_LAST_REACHABLE_BYTES_FOR_COLD_START;
                }

                if total_size > 2 * last_reachable_bytes {
                    if let Err(err) = self.collect_once(tipset).await {
                        warn!("Garbage collection failed: {err}");
                    }
                }
            }
        }
    }

    /// This loop listens on events emitted by `forest-cli db gc` and triggers
    /// `collect_once`
    pub async fn collect_loop_event(self: &Arc<Self>) -> anyhow::Result<()> {
        while let Ok(responder) = self.gc_rx.recv_async().await {
            let this = self.clone();
            let tipset = (self.get_tipset)();
            tokio::spawn(async move {
                let result = this.collect_once(tipset).await;
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
    async fn collect_once(&self, tipset: Tipset) -> anyhow::Result<()> {
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
                if !db.current().has(&cid)? {
                    let pair = (cid.to_bytes(), block.clone());
                    reachable_bytes
                        .fetch_add(pair.0.len() + pair.1.len(), atomic::Ordering::Relaxed);
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

        db.next_current()?;
        Ok(())
    }
}
