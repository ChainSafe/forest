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
//! A correct GC algorithm that is simple and efficient for forest scenarios. This algorithm removes
//! unreachable blocks that are older than `chain finality`, making sure to avoid removing something
//! that could later become reachable as a result of a fork.
//!
//! ## GC Algorithm
//! The `mark-and-sweep` algorithm was chosen due to it's simplicity, efficiency and low memory
//! footprint. Previously the `semi-space` algorithm was used resulting in data duplication and up
//! to a 100% extra disk usage.
//!
//! ## GC Workflow
//! 1. Mark: traverse all the blocks, generating integer hash representations for each identifier
//! and storing those in a set.
//! 2. Wait at least `chain finality` blocks.
//! 3. Traverse reachable blocks starting at the current heaviest tipset and remove those from the
//! marked set, leaving only unreachable entries that are older than `chain finality`.
//! 4. Sweep, removing all the remaining marked entries from the database.
//!
//! ## Correctness
//! This algorithm considers all the blocks that are visited during the `snapshot export` task
//! reachable, making sure they are kept in the database after the run. It makes sure to retain the
//! reachable graph as well as all the blocks for at least `chain finality` to account for potential
//! forks. A snapshot can be used to bootstrap the node from scratch, thus the algorithm is
//! considered correct when a valid snapshot can be exported using records available in the database
//! after the run.
//!
//! ## Disk usage
//! The expected disk usage is slightly greater than the size of live data for three reasons:
//! 1. Unreachable data is not removed until it is at least 7.5 hours old (see `chain finality`).
//! 2. The garbage collector is conservative and is expected to leave a small (less than 1%) amount
//! of unreachable data behind.
//! 3. The blockstore back-end may be fragmented, therefore not relinquishing the disk space back to
//! the OS.
//!
//! ## Memory usage
//! During the `mark` and up to the `sweep` stage, the algorithm requires `4 bytes` of memory for
//! each database record. Additionally, the seen cache while traversing the reachable graph
//! executing the `filter` stage requires at least `32 bytes` of memory for each reachable block.
//! For a typical mainnet snapshot of about 100 GiB that adds up to roughly 2.5 GiB.
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
//! The time complexity of mark and sweep steps is O(n). The filter step is currently utilizing a
//! depth-first search algorithm, with O(V+E) complexity, where V is the number of vertices and E is
//! the number of edges.

use crate::blocks::Tipset;
use crate::chain::ChainEpochDelta;

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

/// [`MarkAndSweep`] is a simple garbage collector implementation that traverses all the database
/// keys writing them to a [`HashSet`], then filters out those that need to be kept and schedules
/// the rest for removal.
///
/// Note: The GC does not know anything about the hybrid CAR-backed and ParityDB approach, only
/// taking care of the latter.
pub struct MarkAndSweep<DB> {
    db: Arc<DB>,
    get_heaviest_tipset: Box<dyn Fn() -> Arc<Tipset> + Send>,
    marked: HashSet<u32>,
    epoch_marked: ChainEpoch,
    depth: ChainEpochDelta,
    block_time: Duration,
}

impl<DB: Blockstore + GarbageCollectable> MarkAndSweep<DB> {
    /// Creates a new mark-and-sweep garbage collector.
    ///
    /// # Arguments
    ///
    /// * `db` - A reference to the database instance.
    /// * `get_heaviest_tipset` - A function that facilitates heaviest tipset retrieval.
    /// * `depth` - The number of state-roots to retain.
    /// * `block_time` - An average block production time.
    pub fn new(
        db: Arc<DB>,
        get_heaviest_tipset: Box<dyn Fn() -> Arc<Tipset> + Send>,
        depth: ChainEpochDelta,
        block_time: Duration,
    ) -> Self {
        Self {
            db,
            get_heaviest_tipset,
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
            self.gc_workflow(interval).await?
        }
    }

    // This function yields to the main GC loop if the conditions are not met for execution of the
    // next step.
    async fn gc_workflow(&mut self, interval: Duration) -> anyhow::Result<()> {
        let depth = self.depth;
        let tipset = (self.get_heaviest_tipset)();
        let current_epoch = tipset.epoch();
        // Don't run the GC if there aren't enough state-roots yet. Sleep and yield to the main loop
        // in order to refresh the heaviest tipset value.
        if depth > current_epoch {
            time::sleep(interval).await;
            return anyhow::Ok(());
        }

        // This signifies a new run.
        if self.marked.is_empty() {
            // Make sure we don't run the GC too often.
            time::sleep(interval).await;

            info!("populate keys for GC");
            self.populate()?;
            self.epoch_marked = current_epoch;
        }

        let epochs_since_marked = current_epoch - self.epoch_marked;
        // Don't proceed with next steps until we advance at least `depth` epochs. Sleep and yield
        // to the main loop in order to refresh the heaviest tipset value.
        if epochs_since_marked < depth {
            time::sleep(self.block_time * (depth - epochs_since_marked) as u32).await;
            return anyhow::Ok(());
        }

        info!("filter keys for GC");
        self.filter(tipset, depth).await?;

        info!("GC sweep");
        self.sweep()?;

        anyhow::Ok(())
    }
}
#[cfg(test)]
mod test {
    use crate::blocks::{BlockHeader, Tipset};
    use crate::chain::{ChainEpochDelta, ChainStore};

    use crate::db::{GarbageCollectable, MarkAndSweep, MemoryDB};
    use crate::message_pool::test_provider::{mock_block, mock_block_with_parents};
    use crate::networks::ChainConfig;

    use crate::utils::db::CborStoreExt;

    use core::time::Duration;

    use crate::shim::clock::ChainEpoch;
    use fvm_ipld_blockstore::Blockstore;
    use std::sync::Arc;

    fn insert_unreachable(db: impl Blockstore, quantity: u64) {
        for idx in 0..quantity {
            let block: BlockHeader = mock_block(1 + idx, 1 + quantity);
            db.put_cbor_default(&block).unwrap();
        }
    }

    fn run_to_epoch(db: impl Blockstore, cs: Arc<ChainStore<MemoryDB>>, epoch: ChainEpoch) {
        let mut heaviest_tipset = cs.heaviest_tipset();

        for _ in heaviest_tipset.epoch()..epoch {
            let block2 = mock_block_with_parents(heaviest_tipset.as_ref(), 1, 1);
            db.put_cbor_default(&block2).unwrap();

            let tipset = Arc::new(Tipset::from(&block2));
            cs.set_heaviest_tipset(tipset).unwrap();
            heaviest_tipset = cs.heaviest_tipset();
        }
    }

    #[tokio::test]
    // This is a test that checks the `mark` step.
    // 1. Generate the genesis block and write it to the database.
    // 2. Try running the GC, encounter insufficient depth, check that there were no marked records.
    // 3. Generate `depth` blocks.
    // 4. Run the GC again to make sure it marked all the available records successfully.
    async fn test_populate() {
        let interval = Duration::from_secs(0);
        let db = Arc::new(MemoryDB::default());
        let chain_config = Arc::new(ChainConfig::default());

        // Generate genesis block.
        let gen_block: BlockHeader = mock_block(1, 1);
        let depth = 1;
        db.put_cbor_default(&gen_block).unwrap();
        let cs = Arc::new(
            ChainStore::new(db.clone(), db.clone(), chain_config, gen_block.clone()).unwrap(),
        );

        let cs_cloned = cs.clone();
        let get_heaviest_tipset = Box::new(move || cs_cloned.heaviest_tipset());
        let mut gc = MarkAndSweep::new(db.clone(), get_heaviest_tipset, depth, interval);

        // test insufficient epochs
        gc.gc_workflow(interval).await.unwrap();
        assert!(gc.marked.is_empty());

        // test marked
        run_to_epoch(db, cs, depth);
        gc.gc_workflow(interval).await.unwrap();
        assert_eq!(gc.marked.len(), 2);
        assert_eq!(gc.epoch_marked, 1);
    }

    #[tokio::test]
    async fn test_filter_and_sweep() {
        let interval = Duration::from_secs(0);
        let db = Arc::new(MemoryDB::default());
        let chain_config = Arc::new(ChainConfig::default());
        let gen_block: BlockHeader = mock_block(1, 1);
        let depth = 1;
        db.put_cbor_default(&gen_block).unwrap();
        let cs = Arc::new(
            ChainStore::new(db.clone(), db.clone(), chain_config, gen_block.clone()).unwrap(),
        );
        let cs_cloned = cs.clone();
        let get_heaviest_tipset = Box::new(move || cs_cloned.heaviest_tipset());
        let mut gc = MarkAndSweep::new(db.clone(), get_heaviest_tipset, depth, interval);

        run_to_epoch(db.clone(), cs.clone(), depth);

        let mut reachable_cnt = (depth + 1) as u64;

        let unreachable_cnt = 4;
        // test insufficient epochs for filter step
        insert_unreachable(db.clone(), unreachable_cnt);
        gc.gc_workflow(interval).await.unwrap();
        assert_eq!(gc.marked.len() as u64, reachable_cnt + unreachable_cnt);
        assert_eq!(gc.epoch_marked, 1);

        // filter and sweep
        run_to_epoch(db.clone(), cs.clone(), depth * 2);
        reachable_cnt += depth as u64;

        assert_eq!(
            db.get_keys().unwrap().len() as u64,
            unreachable_cnt + reachable_cnt
        );
        gc.gc_workflow(interval).await.unwrap();
        assert_eq!(gc.marked.len(), 0);
        assert_eq!(db.get_keys().unwrap().len(), reachable_cnt as usize);

        // try another run
        gc.gc_workflow(interval).await.unwrap();
        assert_eq!(gc.marked.len(), db.get_keys().unwrap().len());
    }

    #[quickcheck_async::tokio]
    async fn test_workflow(depth: u8, current_epoch: u8, unreachable_cnt: u8) {
        let unreachable_cnt = unreachable_cnt as u64;
        // Enforce depth above zero.
        if depth < 1 {
            return;
        }

        // Depth and current epoch are limited to positive numbers to cater for realistic scenarios.
        let depth = depth as ChainEpochDelta;
        let current_epoch = current_epoch as ChainEpoch;

        let interval = Duration::from_secs(0);
        let db = Arc::new(MemoryDB::default());
        let chain_config = Arc::new(ChainConfig::default());
        let gen_block: BlockHeader = mock_block(1, 1);
        db.put_cbor_default(&gen_block).unwrap();
        let cs = Arc::new(
            ChainStore::new(db.clone(), db.clone(), chain_config, gen_block.clone()).unwrap(),
        );
        let cs_cloned = cs.clone();
        let get_heaviest_tipset = Box::new(move || cs_cloned.heaviest_tipset());
        let mut gc = MarkAndSweep::new(db.clone(), get_heaviest_tipset, depth, interval);

        // Make sure we have enough epochs to start garbage collection.
        run_to_epoch(db.clone(), cs.clone(), current_epoch + depth);

        let current_epoch = current_epoch + depth;

        // Insert something to clean up.
        insert_unreachable(db.clone(), unreachable_cnt);

        // Initiate the GC.
        gc.gc_workflow(interval).await.unwrap();
        // Make sure there are marked items.
        assert!(!gc.marked.is_empty());

        run_to_epoch(db.clone(), cs.clone(), current_epoch + depth);
        let current_epoch = current_epoch + depth;

        // Make sure we account for the genesis block.
        let total_reachable_count = current_epoch + 1;

        assert_eq!(
            db.get_keys().unwrap().len() as u64,
            total_reachable_count as u64 + unreachable_cnt
        );

        // filter and sweep
        gc.gc_workflow(interval).await.unwrap();

        assert_eq!(gc.marked.len(), 0);
        assert_eq!(db.get_keys().unwrap().len(), total_reachable_count as usize);
    }
}
