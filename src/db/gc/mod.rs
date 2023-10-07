// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//!
//! The current implementation of the garbage collector is concurrent mark-and-sweep.
//!
//! ## Design goals
//! A correct GC algorithm that is simple and efficient for forest scenarios.
//!
//! ## GC Algorithm
//! The `mark-and-sweep` algorithm was chosen due to it's simplicity, efficiency and low memory
//! footprint. We do have to generate keys from values for some of the iterated columns due to a
//! ParityDB limitation. See <https://github.com/paritytech/parity-db/issues/187>.
//! Previously the `semi-space` algorithm was used resulting in data duplication and up to 100%
//! extra disk usage.
//!
//! ## GC Workflow
//! 1. Mark: traverse all the relevant database columns, generating integer hash representations for
//! each database key and storing those in a [`HashSet`].
//! 2. Wait at least `chain finality` blocks.
//! 3. Traverse reachable blocks starting from the current heaviest tipset and remove those from the
//! marked `HashSet`, leaving only unreachable entries that are older than `chain finality` to avoid
//! removing something that could later become reachable as a result of a fork.
//! 4. Sweep, removing all the remaining marked entries from the database.
//!
//! ## Correctness
//! This algorithm is traversing the reachable graph using the same tooling as the snapshot export.
//! Therefore it ensures that we eliminate all the reachable from the set scheduled for removal.
//! Additionally, it waits at least `chain finality` before filtering out and sweeping marked
//! records, making sure nothing that could have become reachable in the meantime gets removed.
//! A snapshot can be used to bootstrap the node from scratch, therefore the algorithm is considered
//! correct when a valid snapshot can be exported using records available in the database after
//! sweeping.
//!
//! ## Disk usage
//! There's no additional disk space required to run this algorithm.
//!
//! ## Memory usage
//! During the `mark` and up to the `sweep` stage the algorithm requires `4 bytes` of memory for
//! each database record. Additionally, the seen cache while traversing the reachable graph
//! executing the `filter` stage requires at least `32 bytes` of memory for each reachable block.
//!
//! ## Scheduling
//! 1. GC is triggered automatically, there have to be at least `chain finality` epochs stored for
//! the `mark` step.
//! 2. The `filter` step is triggered after at least `chain finality` has passed since `mark` step.
//! 3. Then the `sweep` step happens.
//!
//! ## Performance
//! The GC Performance is calculated by benchmarking the three steps that have to be performed. The
//! `filter` steps consists of two actions: walking the graph and filtering the `marked` set.
//! 1. Traversing all the relevant database records and creating a set of keys. *To be benchmarked*
//! 2. Walking the graph, this has already been benchmarked.
//! `forest-tool benchmark graph-traversal`.
//! WIP: fix and covert this to `unordered-graph-traversal`.
//! 3. Filtering out the records found as a result of `step 2`.
//! 4. Removing all the remaining `marked` records from the database *To be benchmarked*
//!
//! ### Look up performance
//! There should not be any noticeable look up penalty.
//!
//! ### Write performance
//! The only thing that could affect write performance is DB re-index, which should not be affected
//! much by the GC.
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
                    let res = self.gc_workflow().await;
                    msg.send_async(res).await?;
                    info!("finished manual gc run");
                }
                false => {
                    self.gc_workflow().await?;
                    // Make sure we don't run the GC too often.
                    time::sleep(interval).await;
                }
            }
        }
    }

    async fn gc_workflow(&mut self) -> anyhow::Result<()> {
        let depth = self.depth;
        let tipset = self.chain_store.heaviest_tipset();
        let current_epoch = tipset.epoch();
        // Don't run the GC if there aren't enough state-roots yet.
        if depth > current_epoch {
            time::sleep(AVERAGE_BLOCK_TIME * (depth - current_epoch) as u32).await;
            return anyhow::Ok(());
        }

        if self.marked.is_empty() {
            info!("populate keys for GC");
            self.populate()?;
        }

        // Don't filter and sweep before we advance at least `depth`.
        let epoch_since_marked = current_epoch - self.epoch_marked;
        if epoch_since_marked < depth {
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
