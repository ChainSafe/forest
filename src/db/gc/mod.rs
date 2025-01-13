// Copyright 2019-2025 ChainSafe Systems
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
//! Properties:
//!
//! - No `BlockHeader` reachable from HEAD may be garbage collected.
//! - No data younger than `chain finality` epochs may be garbage collected.
//! - State-trees older than `depth` epochs should be garbage collected.
//! - Not all unreachable data has to be garbage collected. In other words, it's
//!   acceptable for the garbage collector to be conservative.
//! - The garbage collector may not prevent access to the database.
//!
//! ## GC Algorithm
//! The `mark-and-sweep` algorithm was chosen due to it's simplicity, efficiency and low memory
//! footprint.
//!
//! ## GC Workflow
//! 1. Mark: traverse all the blocks, generating integer hash representations for each identifier
//!    and storing those in a set.
//! 2. Wait at least `chain finality` blocks.
//! 3. Traverse reachable blocks starting at the current heaviest tipset and remove those from the
//!    marked set, leaving only unreachable entries that are older than `chain finality`.
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
//!    of unreachable data behind.
//! 3. The blockstore back-end may be fragmented, therefore not relinquishing the disk space back to
//!    the OS.
//!
//! ## Memory usage
//! During the `mark` and up to the `sweep` stage, the algorithm requires `4 bytes` of memory for
//! each database record. Additionally, the seen cache while traversing the reachable graph
//! executing the `filter` stage requires at least `32 bytes` of memory for each reachable block.
//! For a typical mainnet snapshot of about 100 GiB that adds up to roughly 2.5 GiB.
//!
//! ## Scheduling
//! 1. GC is triggered automatically and there have to be at least `chain finality` epochs stored
//!    for the `mark` step.
//! 2. The `filter` step is triggered after at least `chain finality` has passed since the `mark`
//!    step.
//! 3. Then, the `sweep` step happens.
//! 4. Finally, the algorithm waits for a configured amount of time to initiate the next run.
//!
//! ## Performance
//! The time complexity of mark and sweep steps is `O(n)`. The filter step is currently utilizing a
//! depth-first search algorithm, with `O(V+E)` complexity, where V is the number of vertices and E
//! is the number of edges.

use crate::blocks::Tipset;
use crate::chain::ChainEpochDelta;

use crate::cid_collections::CidHashSet;
use crate::db::{GarbageCollectable, SettingsStore};
use crate::ipld::stream_graph;
use crate::shim::clock::ChainEpoch;
use futures::StreamExt;
use fvm_ipld_blockstore::Blockstore;
use std::mem;
use std::sync::Arc;
use std::time::Duration;
use tokio::time;
use tracing::{error, info};

const SETTINGS_KEY: &str = "LAST_GC_RUN";

/// [`MarkAndSweep`] is a simple garbage collector implementation that traverses all the database
/// keys writing them to a [`CidHashSet`], then filters out those that need to be kept and schedules
/// the rest for removal.
///
/// Note: The GC does not know anything about the hybrid CAR-backed and ParityDB approach, only
/// taking care of the latter.
pub struct MarkAndSweep<DB> {
    db: Arc<DB>,
    get_heaviest_tipset: Box<dyn Fn() -> Arc<Tipset> + Send>,
    marked: CidHashSet,
    epoch_marked: ChainEpoch,
    depth: ChainEpochDelta,
    block_time: Duration,
}

impl<DB: Blockstore + SettingsStore + GarbageCollectable<CidHashSet> + Sync + Send + 'static>
    MarkAndSweep<DB>
{
    /// Creates a new mark-and-sweep garbage collector.
    ///
    /// # Arguments
    ///
    /// * `db` - A reference to the database instance.
    /// * `get_heaviest_tipset` - A function that facilitates heaviest tipset retrieval.
    /// * `depth` - The number of state-roots to retain. Should be at least `2 * chain finality`.
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
            marked: CidHashSet::new(),
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
        let mut stream = stream_graph(self.db.clone(), tipset.chain_arc(&self.db), depth);
        while let Some(block) = stream.next().await {
            let block = block?;
            self.marked.remove(&block.cid);
        }

        anyhow::Ok(())
    }

    // Remove marked keys from the database.
    fn sweep(&mut self) -> anyhow::Result<u32> {
        let marked = mem::take(&mut self.marked);
        self.db.remove_keys(marked)
    }

    /// Starts the Garbage Collection loop.
    ///
    /// # Arguments
    ///
    /// * `interval` - GC Interval to avoid constantly consuming node's resources.
    ///
    /// NOTE: This currently does not take into account the fact that we might be starting the node
    /// using CAR-backed storage with a snapshot, for implementation simplicity.
    pub async fn gc_loop(&mut self, interval: Duration) -> anyhow::Result<()> {
        loop {
            if let Err(err) = self.gc_workflow(interval).await {
                error!("GC run error: {}", err)
            }
        }
    }

    fn update_last_gc_run(&self, epoch: ChainEpoch) -> anyhow::Result<()> {
        self.db
            .write_bin(SETTINGS_KEY, epoch.to_string().as_bytes())
    }

    // Unfortunately there seems to be no good way of decoding a slice into i64 without array init
    // and manipulation, therefore a string representation is used.
    fn fetch_last_gc_run(&self) -> anyhow::Result<ChainEpoch> {
        let bytes = self.db.read_bin(SETTINGS_KEY)?;
        let epoch = match bytes {
            Some(bytes) => ChainEpoch::from_str_radix(&String::from_utf8(bytes)?, 10)?,
            None => 0,
        };
        Ok(epoch)
    }

    // This function yields to the main GC loop if the conditions are not met for execution of the
    // next step.
    async fn gc_workflow(&mut self, interval: Duration) -> anyhow::Result<()> {
        let depth = self.depth;
        let mut current_tipset = (self.get_heaviest_tipset)();
        let mut current_epoch = current_tipset.epoch();
        let last_gc_run = self.fetch_last_gc_run()?;
        // Don't run the GC if there aren't enough state-roots yet or if we're too close to the last
        // GC run. Sleep and yield to the main loop in order to refresh the heaviest tipset value.
        if depth > current_epoch - last_gc_run {
            time::sleep(interval).await;
            return anyhow::Ok(());
        }

        // This signifies a new run.
        if self.marked.is_empty() {
            // Make sure we don't run the GC too often.
            time::sleep(interval).await;

            // Refresh `current_tipset` and `current_epoch` after sleeping.
            current_tipset = (self.get_heaviest_tipset)();
            current_epoch = current_tipset.epoch();

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
        self.filter(current_tipset, depth).await?;

        info!("GC sweep");
        let deleted = self.sweep()?;
        info!("GC finished sweep: {} deleted records", deleted);

        self.update_last_gc_run(current_epoch)?;

        anyhow::Ok(())
    }
}
#[cfg(test)]
mod test {
    use crate::blocks::{CachingBlockHeader, Tipset};
    use crate::chain::{ChainEpochDelta, ChainStore};
    use crate::db::{GarbageCollectable, MarkAndSweep, MemoryDB, PersistentStore};
    use crate::message_pool::test_provider::{mock_block, mock_block_with_parents};
    use crate::networks::ChainConfig;
    use crate::shim::clock::ChainEpoch;
    use crate::utils::db::CborStoreExt;
    use crate::utils::multihash::prelude::*;
    use cid::Cid;
    use fvm_ipld_blockstore::Blockstore;
    use fvm_ipld_encoding::DAG_CBOR;
    use std::sync::Arc;
    use std::time::Duration;

    const ZERO_DURATION: Duration = Duration::from_secs(0);

    fn insert_unreachable(db: &impl Blockstore, quantity: u64) {
        for idx in 0..quantity {
            let block: CachingBlockHeader = mock_block(1 + idx, 1 + quantity);
            db.put_cbor_default(&block).unwrap();
        }
    }

    fn run_to_epoch(db: &impl Blockstore, cs: &ChainStore<MemoryDB>, epoch: ChainEpoch) {
        let mut heaviest_tipset = cs.heaviest_tipset();

        for _ in heaviest_tipset.epoch()..epoch {
            let block2 = mock_block_with_parents(heaviest_tipset.as_ref(), 1, 1);
            db.put_cbor_default(&block2).unwrap();

            let tipset = Arc::new(Tipset::from(&block2));
            cs.set_heaviest_tipset(tipset).unwrap();
            heaviest_tipset = cs.heaviest_tipset();
        }
    }

    struct GCTester {
        db: Arc<MemoryDB>,
        store: Arc<ChainStore<MemoryDB>>,
    }

    impl GCTester {
        fn new() -> Self {
            let db = Arc::new(MemoryDB::default());
            let config = ChainConfig::default();
            let gen_block: CachingBlockHeader = mock_block(1, 1);
            db.put_cbor_default(&gen_block).unwrap();
            let store = Arc::new(
                ChainStore::new(
                    db.clone(),
                    db.clone(),
                    db.clone(),
                    Arc::new(config),
                    gen_block,
                )
                .unwrap(),
            );

            GCTester { db, store }
        }

        fn run_epochs(&self, delta: ChainEpochDelta) {
            let tipset = self.store.heaviest_tipset();
            let epoch = tipset.epoch() + delta;
            run_to_epoch(&self.db, &self.store, epoch);
        }

        fn insert_unreachable(&self, block_number: i64) {
            insert_unreachable(&self.db, block_number as u64);
        }

        fn get_heaviest_tipset_fn(&self) -> Box<dyn Fn() -> Arc<Tipset> + Send> {
            let store = self.store.clone();
            Box::new(move || store.heaviest_tipset())
        }
    }

    // This is a test that checks the `mark` step.
    // 1. Generate the genesis block and write it to the database.
    // 2. Try running the GC, encounter insufficient depth, check that there were no marked records.
    // 3. Generate `depth` blocks.
    // 4. Run the GC again to make sure it marked all the available records successfully.
    #[quickcheck_async::tokio]
    async fn test_populate(depth: u8) {
        // Enforce depth above zero.
        if depth < 1 {
            return;
        }

        let tester = GCTester::new();
        let depth = depth as ChainEpochDelta;

        let mut gc = MarkAndSweep::new(
            tester.db.clone(),
            tester.get_heaviest_tipset_fn(),
            depth,
            ZERO_DURATION,
        );

        // test insufficient epochs
        gc.gc_workflow(ZERO_DURATION).await.unwrap();
        assert!(gc.marked.is_empty());

        // test marked
        tester.run_epochs(depth);
        gc.gc_workflow(ZERO_DURATION).await.unwrap();
        assert_eq!(gc.marked.len(), 1 + depth as usize);
        assert_eq!(gc.epoch_marked, depth);
    }

    // TODO(forest): https://github.com/ChainSafe/forest/issues/4404
    // #[quickcheck_async::tokio]
    #[allow(dead_code)]
    async fn dont_gc_reachable_data(depth: u8, current_epoch: u8) {
        // Enforce depth above zero.
        if depth < 1 {
            return;
        }

        let depth = depth as ChainEpochDelta;
        let current_epoch = current_epoch as ChainEpochDelta;

        let tester = GCTester::new();
        let mut gc = MarkAndSweep::new(
            tester.db.clone(),
            tester.get_heaviest_tipset_fn(),
            depth,
            ZERO_DURATION,
        );

        let depth = depth as ChainEpochDelta;
        let current_epoch = current_epoch as ChainEpochDelta;

        // Make sure we run enough epochs to initiate GC.
        tester.run_epochs(current_epoch);
        tester.run_epochs(depth);
        // Mark.
        gc.gc_workflow(ZERO_DURATION).await.unwrap();
        tester.run_epochs(depth);
        // Sweep.
        gc.gc_workflow(ZERO_DURATION).await.unwrap();

        // Make sure we don't clean anything up.
        assert_eq!(
            tester.db.get_keys().unwrap().len() as i64,
            // `Current epoch + genesis block + twice the depth.`
            current_epoch + 1 + depth * 2
        );
    }

    // TODO(forest): https://github.com/ChainSafe/forest/issues/4404
    // #[quickcheck_async::tokio]
    #[allow(dead_code)]
    async fn no_young_data_cleanups(depth: u8, current_epoch: u8, unreachable_nodes: u8) {
        // Enforce depth above zero.
        if depth < 1 {
            return;
        }

        let depth = depth as ChainEpochDelta;
        let current_epoch = current_epoch as ChainEpochDelta;
        let unreachable_nodes = unreachable_nodes as i64;

        let tester = GCTester::new();
        let mut gc = MarkAndSweep::new(
            tester.db.clone(),
            tester.get_heaviest_tipset_fn(),
            depth,
            ZERO_DURATION,
        );

        let depth = depth as ChainEpochDelta;
        let current_epoch = current_epoch as ChainEpochDelta;

        // Make sure we run enough epochs to initiate GC.
        tester.run_epochs(current_epoch);
        tester.run_epochs(depth);
        // Mark.
        gc.gc_workflow(ZERO_DURATION).await.unwrap();
        tester.run_epochs(depth);

        // Insert unreachable nodes after the mark step.
        tester.insert_unreachable(unreachable_nodes);
        // Sweep.
        gc.gc_workflow(ZERO_DURATION).await.unwrap();

        // Make sure we don't clean anything up.
        assert_eq!(
            tester.db.get_keys().unwrap().len() as i64,
            // `Current epoch + genesis block + twice the depth + unreachable nodes.`
            current_epoch + 1 + depth * 2 + unreachable_nodes
        );
    }

    // TODO(forest): https://github.com/ChainSafe/forest/issues/4404
    // #[quickcheck_async::tokio]
    #[allow(dead_code)]
    async fn unreachable_old_data_collected(depth: u8, current_epoch: u8, unreachable_nodes: u8) {
        // Enforce depth above zero.
        if depth < 1 {
            return;
        }

        let depth = depth as ChainEpochDelta;
        let current_epoch = current_epoch as ChainEpochDelta;
        let unreachable_nodes = unreachable_nodes as i64;

        let tester = GCTester::new();
        let mut gc = MarkAndSweep::new(
            tester.db.clone(),
            tester.get_heaviest_tipset_fn(),
            depth,
            ZERO_DURATION,
        );

        let depth = depth as ChainEpochDelta;
        let current_epoch = current_epoch as ChainEpochDelta;

        // Make sure we run enough epochs to initiate GC.
        tester.run_epochs(current_epoch);
        tester.run_epochs(depth);
        // Insert unreachable nodes before the mark step.
        tester.insert_unreachable(unreachable_nodes);
        // Mark.
        gc.gc_workflow(ZERO_DURATION).await.unwrap();
        tester.run_epochs(depth);

        // Sweep.
        gc.gc_workflow(ZERO_DURATION).await.unwrap();

        // Make sure we clean up old unreachable data.
        assert_eq!(
            tester.db.get_keys().unwrap().len() as i64,
            // `Current epoch + genesis block + twice the depth.`
            current_epoch + 1 + depth * 2
        );
    }

    #[tokio::test]
    async fn persistent_data_resilient_to_gc() {
        let depth = 5 as ChainEpochDelta;
        let current_epoch = 0 as ChainEpochDelta;

        let tester = GCTester::new();
        let mut gc = MarkAndSweep::new(
            tester.db.clone(),
            tester.get_heaviest_tipset_fn(),
            depth,
            ZERO_DURATION,
        );

        let depth = depth as ChainEpochDelta;
        let current_epoch = current_epoch as ChainEpochDelta;

        let persistent_data = [1, 55];
        let persistent_cid =
            Cid::new_v1(DAG_CBOR, MultihashCode::Identity.digest(&persistent_data));

        // Make sure we run enough epochs to initiate GC.
        tester.run_epochs(current_epoch);
        tester.run_epochs(depth);
        tester
            .db
            .put_keyed_persistent(&persistent_cid, &persistent_data)
            .unwrap();
        // Mark.
        gc.gc_workflow(ZERO_DURATION).await.unwrap();
        tester.run_epochs(depth);
        // Sweep.
        gc.gc_workflow(ZERO_DURATION).await.unwrap();

        // Make sure persistent data stays.
        assert_eq!(
            tester.db.get(&persistent_cid).unwrap(),
            Some(persistent_data.to_vec())
        );
    }
}
