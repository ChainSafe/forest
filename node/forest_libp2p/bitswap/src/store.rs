use crate::*;
use libipld::Block;

/// Trait implemented by a block store.
pub trait BitswapStore: Send + Sync + 'static {
    /// The store params.
    type Params: StoreParams;
    /// A have query needs to know if the block store contains the block.
    fn contains(&self, cid: &Cid) -> anyhow::Result<bool>;
    /// A block query needs to retrieve the block from the store.
    fn get(&self, cid: &Cid) -> anyhow::Result<Option<Vec<u8>>>;
    /// A block response needs to insert the block into the store.
    fn insert(&self, block: &Block<Self::Params>) -> anyhow::Result<()>;
    /// A sync query needs a list of missing blocks to make progress.
    fn missing_blocks(&self, cid: &Cid) -> anyhow::Result<Vec<Cid>>;
}
