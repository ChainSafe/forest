// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//!
//! The current implementation of the garbage collector is `concurrent mark-and-sweep`.
//!
//! ## Terminology
//! `chain finality` - a number of epochs after which it becomes impossible to add or remove a block
//! previously appended to the blockchain.
//!
//! ## Design goals
//! A correct GC algorithm that is simple and efficient for forest scenarios.
//!
//! ## GC Algorithm
//! The `mark-and-sweep` algorithm was chosen due to it's simplicity, efficiency and low memory
//! footprint. Previously the `semi-space` algorithm was used resulting in data duplication and up
//! to a 100% extra disk usage.
//!
//! ## GC Workflow
//! 1. Mark: traverse all the relevant database columns, generating integer hash representations for
//! each database key and storing those in a set.
//! 2. Wait at least `chain finality` blocks.
//! 3. Traverse reachable blocks starting at the current heaviest tipset and remove those from the
//! marked set, leaving only unreachable entries that are older than `chain finality` to avoid
//! removing something that could later become reachable as a result of a fork.
//! 4. Sweep, removing all the remaining marked entries from the database.
//!
//! ## Correctness
//! This algorithm considers all the blocks that are visited during the `snapshot export` task
//! reachable, making sure they are kept in the database after the run. This is facilitated by
//! waiting at least `chain finality` after the `mark` step before traversing the reachable graph
//! and eliminating all the found blocks from the set marked for removal. A snapshot can be used to
//! bootstrap the node from scratch, thus the algorithm is considered correct when a valid snapshot
//! can be exported using records available in the database after sweeping.
//!
//! ## Disk usage
//! There's no additional disk space required to run this algorithm. However, removing the
//! unreachable blocks from the database takes at least `chain finality`. The GC speed depends on
//! the reachable graph size. Disk usage is also affected by the GC run interval. Additionally,
//! since a truncated 4-byte hash is used - there's a slight possibility of a collision, which might
//! result in an unreachable block being retained in the database. Still, the impact on the total
//! disk size is negligible.
//!
//! ## Memory usage
//! During the `mark` and up to the `sweep` stage, the algorithm requires `4 bytes` of memory for
//! each database record. Additionally, the seen cache while traversing the reachable graph
//! executing the `filter` stage requires at least `32 bytes` of memory for each reachable block.
//!
//! ## Scheduling
//! 1. GC is triggered automatically and there have to be at least `chain finality` epochs stored
//! for the `mark` step.
//! 2. The `filter` step is triggered after at least `chain finality` has passed since the `mark`
//! step.
//! 3. Then, the `sweep` step happens.
//! 4. Finally, the algorithm waits for a configured amount of time to initiate the next run.
//!
//! ## Performance
//! TODO: Measure the performance and potentially define it in terms of `snapshot export` or any
//! other visible and comparable metric.
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

// This enum facilitates GC loop control flow. It allows for simpler workflow logic by avoiding
// inner loops within the workflow itself.
enum ControlFlow {
    // Continue the GC workflow.
    Continue,
    // The GC workflow has finished, start another one.
    Finished,
}

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
    depth: ChainEpochDelta,
    block_time: Duration,
}

impl<BS: Blockstore> MarkAndSweep<BS> {
    /// Creates a new mark-and-sweep garbage collector.
    ///
    /// # Arguments
    ///
    /// * `db` - A reference to the database instance.
    /// * `chain_store` - A reference to chain store to fetch heaviest tipset.
    /// * `depth` - The number of state-roots to retain.
    /// * `block_time` - An average block production time.
    pub fn new(
        db: Arc<Db>,
        chain_store: Arc<ChainStore<BS>>,
        depth: ChainEpochDelta,
        block_time: Duration,
    ) -> Self {
        Self {
            db,
            chain_store,
            depth,
            marked: HashSet::new(),
            epoch_marked: 0,
            block_time,
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
    ///
    /// NOTE: This currently does not take into account the fact that we might be starting the node
    /// using CAR-backed storage with a snapshot, for implementation simplicity.
    pub async fn gc_loop(&mut self, interval: Duration) -> anyhow::Result<()> {
        loop {
            match self.gc_workflow().await? {
                ControlFlow::Continue => continue,
                ControlFlow::Finished => {
                    // Make sure we don't run the GC too often.
                    time::sleep(interval).await;
                }
            }
        }
    }

    async fn gc_workflow(&mut self) -> anyhow::Result<ControlFlow> {
        let depth = self.depth;
        let tipset = self.chain_store.heaviest_tipset();
        let current_epoch = tipset.epoch();
        // Don't run the GC if there aren't enough state-roots yet.
        if depth > current_epoch {
            time::sleep(self.block_time * (depth - current_epoch) as u32).await;
            return anyhow::Ok(ControlFlow::Continue);
        }

        if self.marked.is_empty() {
            info!("populate keys for GC");
            self.populate()?;
        }

        let epochs_since_marked = current_epoch - self.epoch_marked;
        if epochs_since_marked < depth {
            time::sleep(self.block_time * (depth - epochs_since_marked) as u32).await;
            return anyhow::Ok(ControlFlow::Continue);
        }

        info!("filter keys for GC");
        self.filter(tipset, depth).await?;

        info!("GC sweep");
        self.sweep()?;

        anyhow::Ok(ControlFlow::Finished)
    }
}
#[cfg(test)]
mod test {
    // fn
}
