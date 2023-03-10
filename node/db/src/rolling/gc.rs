// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

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
            if let (Ok(total_size), Ok(current_size)) = (
                self.db.total_size_in_bytes(),
                self.db.current_size_in_bytes(),
            ) {
                // Collect when size of young partition > 0.5 * size of old partition
                if total_size > 0 && self.db.db_count() > 1 && current_size * 3 > total_size {
                    if let Err(err) = self.collect_once().await {
                        warn!("Garbage collection failed: {err}");
                    }
                }
            }

            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    }

    pub async fn collect_loop_event(self: &Arc<Self>) -> anyhow::Result<()> {
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

    async fn collect_once(&self) -> anyhow::Result<()> {
        let guard = self.lock.try_lock();
        if guard.is_err() {
            anyhow::bail!("Another garbage collection task is in progress.");
        }

        let start = Utc::now();
        let tipset = (self.get_tipset)();
        info!("Garbage collection started at epoch {}", tipset.epoch());
        let db = &self.db;
        if db.db_count() > 1 {
            // 128MB
            const BUFFER_CAPCITY_BYTES: usize = 128 * 1024 * 1024;
            let (tx, rx) = flume::unbounded();
            let write_task = tokio::spawn({
                let db = db.current();
                async move { db.buffered_write(rx, BUFFER_CAPCITY_BYTES).await }
            });
            walk_snapshot(&tipset, DEFAULT_RECENT_ROOTS, |cid| {
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
        }
        info!(
            "Garbage collection finished at epoch {}, took {}s",
            tipset.epoch(),
            (Utc::now() - start).num_seconds()
        );
        db.clean_tracked(1, true)?;
        db.next_partition()?;
        db.clean_untracked()?;
        Ok(())
    }
}
