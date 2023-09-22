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
use fvm_ipld_blockstore::Blockstore;
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
    /// Creates a new MarkAndSweep garbage collector.
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
    /// * `manual` - Whether or not the GC has to be triggered manually or automatically.
    ///
    /// NOTE: This currently does not take into account the fact that we might be starting the node
    /// using CAR-backed storage with a snapshot, for implementation simplicity.
    pub async fn gc_loop(&mut self, interval: Duration, manual: bool) -> anyhow::Result<()> {
        loop {
            match manual {
                true => {
                    let msg = self.gc_receiver.recv_async().await?;
                    let res = self.gc_workflow(interval).await;
                    msg.send(res)?
                }
                false => {
                    self.gc_workflow(interval).await?;

                    // Make sure we don't run the GC too often.
                    time::sleep(interval).await;
                }
            }
        }
    }

    async fn gc_workflow(&mut self, interval: Duration) -> anyhow::Result<()> {
        let depth = self.depth;
        let tipset = self.chain_store.heaviest_tipset();
        let current_epoch = tipset.epoch();
        // Don't run the GC if there aren't enough state-roots yet.
        if depth > current_epoch {
            time::sleep(AVERAGE_BLOCK_TIME * (depth - current_epoch) as u32).await;
            return anyhow::Ok(());
        }

        // Make sure we don't GC if we haven't advanced at least `depth` number of epochs since
        // the last sweep.
        let epochs_since_gc = current_epoch - self.epoch_sweeped;
        if self.marked.is_empty() && epochs_since_gc < depth {
            time::sleep(AVERAGE_BLOCK_TIME * epochs_since_gc as u32).await;
            return anyhow::Ok(());
        } else {
            self.populate()?;
        }

        // Don't filter and sweep before we advance at least `depth`.
        let epoch_since_marked = current_epoch - self.epoch_marked;
        if !self.marked.is_empty() && epoch_since_marked < depth {
            time::sleep(AVERAGE_BLOCK_TIME * epoch_since_marked as u32).await;
            return anyhow::Ok(());
        } else {
            self.filter(tipset, depth)?;
            self.sweep()?;
            self.epoch_sweeped = current_epoch;
        }

        return anyhow::Ok(());
    }
}
