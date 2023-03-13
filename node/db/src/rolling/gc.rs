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
//! 1. GC is triggered automatically when current DB size is greater than 50% of
//! the old DB size
//! 2. GC can be triggered manually by `forest-cli db gc` command
//! 3. There's global GC lock to ensure at most one GC job is running

use std::time::Duration;

use chrono::Utc;
use forest_blocks::Tipset;
use forest_ipld::util::*;
use fvm_ipld_blockstore::Blockstore;
use tokio::sync::Mutex;

use super::*;
use crate::{Store, StoreExt};

pub struct DbGarbageCollector<F>
where
    F: Fn() -> Tipset + Send + Sync + 'static,
{
    db: RollingDB,
    get_tipset: F,
    lock: Mutex<()>,
    gc_tx: flume::Sender<flume::Sender<anyhow::Result<()>>>,
    gc_rx: flume::Receiver<flume::Sender<anyhow::Result<()>>>,
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
        }
    }

    pub fn get_tx(&self) -> flume::Sender<flume::Sender<anyhow::Result<()>>> {
        self.gc_tx.clone()
    }

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

            if let (Ok(total_size), Ok(current_size)) = (
                self.db.total_size_in_bytes(),
                self.db.current_size_in_bytes(),
            ) {
                // Collect when size of young partition > 0.5 * size of old partition
                if total_size > 0 && current_size * 3 > total_size {
                    if let Err(err) = self.collect_once(tipset).await {
                        warn!("Garbage collection failed: {err}");
                    }
                }
            }
        }
    }

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

    async fn collect_once(&self, tipset: Tipset) -> anyhow::Result<()> {
        let guard = self.lock.try_lock();
        if guard.is_err() {
            anyhow::bail!("Another garbage collection task is in progress.");
        }

        let start = Utc::now();

        info!("Garbage collection started at epoch {}", tipset.epoch());
        let db = &self.db;
        // 128MB
        const BUFFER_CAPCITY_BYTES: usize = 128 * 1024 * 1024;
        let (tx, rx) = flume::bounded(100);
        let write_task = tokio::spawn({
            let db = db.current();
            async move { db.buffered_write(rx, BUFFER_CAPCITY_BYTES).await }
        });
        walk_snapshot(&tipset, DEFAULT_RECENT_STATE_ROOTS, |cid| {
            let db = db.clone();
            let tx = tx.clone();
            async move {
                let block = db
                    .get(&cid)?
                    .ok_or_else(|| anyhow::anyhow!("Cid {cid} not found in blockstore"))?;
                if !db.current().has(&cid)? {
                    tx.send_async((cid.to_bytes(), block.clone())).await?;
                }

                Ok(block)
            }
        })
        .await?;
        drop(tx);
        write_task.await??;

        info!(
            "Garbage collection finished at epoch {}, took {}s",
            tipset.epoch(),
            (Utc::now() - start).num_seconds()
        );
        db.next_partition()?;
        Ok(())
    }
}
