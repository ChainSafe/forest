use crate::blocks::Tipset;
use crate::chain::{ChainEpochDelta, ChainStore};
use crate::db::db_engine::Db;
use crate::db::parity_db::ParityDb;
use crate::db::{truncated_hash, GarbageCollectable};
use crate::ipld::{
    stream_chain, unordered_stream_chain, unordered_stream_graph, ChainStream, CidHashSet,
};
use crate::shim::clock::ChainEpoch;
use ahash::{HashSet, HashSetExt};
use futures::StreamExt;
use std::mem;
use std::sync::Arc;
use std::time::Duration;
use tokio::time;

// The average block time is set to a slightly bigger value on purpose, to save some cycles.
const AVERAGE_BLOCK_TIME: Duration = Duration::from_secs(35);

/// [`MarkAndSweep`] is a simple garbage collector implementation that traverses all the database
/// keys writing them to a [`HashSet`], then filters out those that need to be kept and schedules
/// the rest for removal.
///
/// Note: The GC does not know anything about the hybrid CAR-backed + ParityDB approach, only taking
/// care of the latter.
pub struct MarkAndSweep {
    db: Arc<Db>,
    chain_store: Arc<ChainStore<Db>>,
    marked: HashSet<u32>,
    epoch_marked: ChainEpoch,
}

impl MarkAndSweep {
    pub fn new(db: Arc<Db>, chain_store: Arc<ChainStore<Db>>) -> Self {
        Self {
            db,
            chain_store,
            marked: HashSet::new(),
            epoch_marked: 0,
        }
    }
    // Populate the initial set with all the available database keys.
    fn populate(&mut self) -> anyhow::Result<()> {
        self.marked = self.db.get_keys()?;
        Ok(())
    }

    // Filter out the initial set, leaving only the entries that need to be removed.
    // NOTE: One concern here is that this is going to consume a lot of CPU.
    fn filter(&mut self, tipset: Arc<Tipset>, depth: ChainEpochDelta) -> anyhow::Result<()> {
        // NOTE: We want to keep all the block headers from genesis to heaviest tipset epoch.
        let mut stream = unordered_stream_graph(
            self.db.clone(),
            (*tipset).clone().chain(self.db.clone()),
            depth,
        );

        while let Some(block) = futures::executor::block_on(stream.next()) {
            let block = block?;
            self.marked.remove(&truncated_hash(&block.cid.hash()));
        }

        anyhow::Ok(())
    }

    // Remove marked keys from the database.
    fn sweep(&mut self) -> anyhow::Result<()> {
        let mut marked = HashSet::new();
        mem::swap(&mut marked, &mut self.marked);
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
    pub async fn gc_loop(
        &mut self,
        depth: ChainEpochDelta,
        interval: Duration,
    ) -> anyhow::Result<()> {
        let mut last_sweeped: ChainEpoch = 0;
        loop {
            let tipset = self.chain_store.heaviest_tipset();
            let current_epoch = tipset.epoch();
            // Don't run the GC if there aren't enough state-roots yet.
            if depth > current_epoch {
                time::sleep(AVERAGE_BLOCK_TIME * (depth - current_epoch) as u32).await;
                continue;
            }

            // Make sure we don't GC if we haven't advanced at least `depth` number of epochs since
            // the last sweep.
            let epochs_since_gc = current_epoch - last_sweeped;
            if self.marked.is_empty() && epochs_since_gc < depth {
                time::sleep(AVERAGE_BLOCK_TIME * epochs_since_gc as u32).await;
                continue;
            } else {
                self.populate()?;
            }

            // Don't filter and sweep before we advance at least `depth`.
            let epoch_since_marked = current_epoch - self.epoch_marked;
            if !self.marked.is_empty() && epoch_since_marked < depth {
                time::sleep(AVERAGE_BLOCK_TIME * epoch_since_marked as u32).await;
                continue;
            } else {
                self.filter(tipset, depth)?;
                self.sweep()?;
                last_sweeped = current_epoch;
            }

            // Make sure we don't run the GC too often.
            time::sleep(interval).await;
        }
    }
}
