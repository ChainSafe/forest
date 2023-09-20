use crate::blocks::Tipset;
use crate::chain::ChainEpochDelta;
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

/// [`MarkAndSweep`] is a simple garbage collector implementation that traverses all the database
/// keys writing them to a [`CidHashSet`], then filters out those that need to be kept and schedules
/// the rest for removal.
///
/// TODO: This likely has to take into account the new design that combines CAR-backed store and
/// ParityDB.
pub struct MarkAndSweep {
    db: Arc<Db>,
    marked: HashSet<u32>,
    epoch_marked: ChainEpoch,
}

impl MarkAndSweep {
    // Populate the initial set with all the available database keys.
    fn populate(&mut self) -> anyhow::Result<()> {
        self.marked = self.db.get_keys()?;
        Ok(())
    }

    // Filter out the initial set, leaving only the entries that need to be removed.
    // NOTE: One concern here is that this is going to consume a lot of CPU.
    fn filter(&mut self, tipset: Tipset, depth: ChainEpochDelta) -> anyhow::Result<()> {
        // TODO: Figure out if we need a special case here in order to avoid emitting blocks
        // discovered from tipset iteration, where tipset.epoch() <= stateroot_limit. Right now
        // we emit every block of each discovered tipset.
        let mut stream =
            unordered_stream_graph(self.db.clone(), tipset.chain(self.db.clone()), depth);

        while let Some(block) = futures::executor::block_on(stream.next()) {
            let block = block?;
            self.marked.remove(&truncated_hash(&block.cid.hash()));
        }

        anyhow::Ok(())
    }

    fn sweep(&mut self) -> anyhow::Result<()> {
        let mut marked = HashSet::new();
        mem::swap(&mut marked, &mut self.marked);
        self.db.remove_keys(marked)
    }
}
