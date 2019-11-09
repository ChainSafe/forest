use crate::block_header::{BlockHeader, BlockCID, ChainWeight, ChainEpoch};
pub struct Tipset{
    block_CID: Vec<BlockCID>,
    blocks: Vec<BlockHeader>,

    parents: Option<Box<Tipset>>,
    //StateTree
    weights: ChainWeight,
    epoch: ChainEpoch,
    
}