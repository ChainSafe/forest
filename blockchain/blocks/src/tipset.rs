use crate::block_header::{BlockCID, BlockHeader, ChainEpoch, ChainWeight};
pub struct Tipset {
    block_cid: Vec<BlockCID>,
    blocks: Vec<BlockHeader>,

    parents: Option<Box<Tipset>>,
    //StateTree
    weights: ChainWeight,
    epoch: ChainEpoch,
}
