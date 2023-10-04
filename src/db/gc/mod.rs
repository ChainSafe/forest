// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use crate::blocks::Tipset;
use crate::chain::{ChainEpochDelta, ChainStore};
use crate::db::db_engine::Db;

use crate::db::{truncated_hash, GarbageCollectable};
use crate::ipld::stream_graph;
use crate::shim::clock::ChainEpoch;
use ahash::{HashSet, HashSetExt};
use futures::StreamExt;
use fvm_ipld_blockstore::Blockstore;
use std::mem;
use std::sync::Arc;
use std::time::Duration;
use tokio::time;
use tracing::info;

// The average block time is set to a slightly bigger value on purpose, to save some cycles.
const AVERAGE_BLOCK_TIME: Duration = Duration::from_secs(35);

/// [`MarkAndSweep`] is a simple garbage collector implementation that traverses all the database
/// keys writing them to a [`HashSet`], then filters out those that need to be kept and schedules
/// the rest for removal.
///
/// Note: The GC does not know anything about the hybrid CAR-backed and ParityDB approach, only
/// taking care of the latter.
pub struct MarkAndSweep<BS> {
    db: Arc<Db>,
    chain_store: Arc<ChainStore<BS>>,
    marked: HashSet<u32>,
    epoch_marked: ChainEpoch,
    epoch_sweeped: ChainEpoch,
    depth: ChainEpochDelta,
    gc_receiver: flume::Receiver<flume::Sender<anyhow::Result<()>>>,
}

impl<BS: Blockstore> MarkAndSweep<BS> {
    /// Creates a new mark-and-sweep garbage collector.
    ///
    /// # Arguments
    ///
    /// * `db` - A reference to the database instance.
    /// * `chain_store` - A reference to chain store to fetch heaviest tipset.
    /// * `depth` - The number of state-roots to retain.
    /// * `gc_receiver` - A channel for manually triggering the GC and sending the result back.
    pub fn new(
        db: Arc<Db>,
        chain_store: Arc<ChainStore<BS>>,
        depth: ChainEpochDelta,
        gc_receiver: flume::Receiver<flume::Sender<anyhow::Result<()>>>,
    ) -> Self {
        Self {
            db,
            chain_store,
            depth,
            marked: HashSet::new(),
            epoch_marked: 0,
            epoch_sweeped: 0,
            gc_receiver,
        }
    }
    // Populate the initial set with all the available database keys.
    fn populate(&mut self) -> anyhow::Result<()> {
        self.marked = self.db.get_keys()?;
        Ok(())
    }

    // Filter out the initial set, leaving only the entries that need to be removed.
    // NOTE: One concern here is that this is going to consume a lot of CPU.
    async fn filter(&mut self, tipset: Arc<Tipset>, depth: ChainEpochDelta) -> anyhow::Result<()> {
        // NOTE: We want to keep all the block headers from genesis to heaviest tipset epoch.
        let mut stream = stream_graph(
            self.db.clone(),
            (*tipset).clone().chain(self.db.clone()),
            depth,
        );

        while let Some(block) = stream.next().await {
            let block = block?;
            self.marked.remove(&truncated_hash(block.cid.hash()));
        }

        anyhow::Ok(())
    }

    // Remove marked keys from the database.
    fn sweep(&mut self) -> anyhow::Result<()> {
        let marked = mem::take(&mut self.marked);
        self.db.remove_keys(marked)
    }

    /// Starts the Garbage Collection loop.
    ///
    /// # Arguments
    ///
    /// * `depth` - Specifies how far back the full history should be maintained. Cannot be less
    /// than chain finality.
    /// * `interval` - GC Interval to avoid constantly consuming node's resources.
    /// * `manual` - Whether or not the GC has to be triggered manually or automatically.
    ///
    /// NOTE: This currently does not take into account the fact that we might be starting the node
    /// using CAR-backed storage with a snapshot, for implementation simplicity.
    pub async fn gc_loop(&mut self, interval: Duration, manual: bool) -> anyhow::Result<()> {
        loop {
            match manual {
                true => {
                    let msg = self.gc_receiver.recv_async().await?;
                    info!("running manual gc");
                    let res = self.gc_workflow(manual).await;
                    msg.send_async(res).await?;
                    info!("finished manual gc run");
                }
                false => {
                    self.gc_workflow(manual).await?;
                    // Make sure we don't run the GC too often.
                    time::sleep(interval).await;
                }
            }
        }
    }

    async fn gc_workflow(&mut self, manual: bool) -> anyhow::Result<()> {
        let depth = self.depth;
        let tipset = self.chain_store.heaviest_tipset();
        let current_epoch = tipset.epoch();
        // Don't run the GC if there aren't enough state-roots yet. Manual GC overrides that.
        if !manual || depth > current_epoch {
            time::sleep(AVERAGE_BLOCK_TIME * (depth - current_epoch) as u32).await;
            return anyhow::Ok(());
        }

        // Make sure we don't GC if we haven't advanced at least `depth` number of epochs since
        // the last sweep. Manual GC overrides that.
        let epochs_since_gc = current_epoch - self.epoch_sweeped;
        if self.marked.is_empty() && !manual && epochs_since_gc < depth {
            time::sleep(AVERAGE_BLOCK_TIME * epochs_since_gc as u32).await;
            return anyhow::Ok(());
        } else {
            info!("populate keys for GC");
            self.populate()?;
        }

        // Nothing to do.
        if self.marked.is_empty() {
            return anyhow::Ok(());
        }

        // Don't filter and sweep before we advance at least `depth`. Manual GC overrides that.
        let epoch_since_marked = current_epoch - self.epoch_marked;
        if !manual && epoch_since_marked < depth {
            time::sleep(AVERAGE_BLOCK_TIME * epoch_since_marked as u32).await;
            return anyhow::Ok(());
        } else {
            info!("filter keys for GC");
            self.filter(tipset, depth).await?;

            info!("GC sweep");
            self.sweep()?;
            self.epoch_sweeped = current_epoch;
        }

        anyhow::Ok(())
    }
}
