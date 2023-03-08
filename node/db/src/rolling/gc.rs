// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::time::Duration;

use chrono::Utc;
use forest_blocks::Tipset;
use forest_ipld::util::*;
use fvm_ipld_blockstore::Blockstore;
use tokio::sync::Mutex;

use super::*;
use crate::Store;

pub struct DbGarbageCollector<F: Fn() -> Tipset> {
    db: RollingDB,
    get_tipset: F,
    lock: Mutex<()>,
}

impl<F: Fn() -> Tipset> DbGarbageCollector<F> {
    pub fn new(db: RollingDB, get_tipset: F) -> Self {
        Self {
            db,
            get_tipset,
            lock: Default::default(),
        }
    }

    pub async fn collect_loop(&self) -> anyhow::Result<()> {
        loop {
            if let Ok(total_size) = self.db.total_size_in_bytes() {
                if let Ok(current_size) = self.db.current_size_in_bytes() {
                    if total_size > 0 && self.db.db_count() > 1 && current_size * 3 > total_size {
                        if let Err(err) = self.collect_once().await {
                            warn!("Garbage collection failed: {err}");
                        }
                    }
                }
            }
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    }

    pub async fn collect_once(&self) -> anyhow::Result<()> {
        if self.lock.try_lock().is_ok() {
            let start = Utc::now();
            let tipset = (self.get_tipset)();
            info!("Garbage collection started at epoch {}", tipset.epoch());
            let db = &self.db;
            walk_snapshot(&tipset, DEFAULT_RECENT_ROOTS, |cid| {
                let db = db.clone();
                async move {
                    let block = db
                        .get(&cid)?
                        .ok_or_else(|| anyhow::anyhow!("Cid {cid} not found in blockstore"))?;
                    if should_save_block_to_snapshot(&cid) && !db.current().has(&cid)? {
                        db.current().put_keyed(&cid, &block)?;
                    }

                    Ok(block)
                }
            })
            .await?;
            info!(
                "Garbage collection finished at epoch {}, took {}s",
                tipset.epoch(),
                (Utc::now() - start).num_seconds()
            );
            db.clean_tracked(1, true)?;
            db.next_partition()?;
            db.clean_untracked()?;
            Ok(())
        } else {
            anyhow::bail!("Another garbage collection task is in progress.");
        }
    }
}
