#![allow(unused_variables)]
#![allow(dead_code)]

use cid::Cid;

use super::block::BlockHeader;
use super::ticket::Ticket;

use super::errors::Error;

/// TipSet is an immutable set of blocks at the same height with the same parent set
/// Blocks in a tipset are canonically ordered by ticket size
pub struct Tipset {
    blocks: Vec<BlockHeader>,
    key: TipSetKeys,
}

/// TipSetKeys is a set of CIDs forming a unique key for a TipSet
/// Equal keys will have equivalent iteration order, but note that the CIDs are *not* maintained in
/// the same order as the canonical iteration order of blocks in a tipset (which is by ticket)
#[derive(Clone)]
pub struct TipSetKeys {
    pub cids: Vec<Cid>,
}

impl Tipset {
    /// new builds a new TipSet from a collection of blocks
    //// The blocks must be distinct (different CIDs), have the same height, and same parent set
    fn new(headers: Vec<BlockHeader>) -> Result<Self, Error> {
        // TODO
        // check length of blocks is not 0
        // loop through headers to ensure blocks have same height and parent set
        // sort headers by ticket size
        // check and assign uniqueness of key
        // return TipSet type
        if headers.is_empty() {
            return Err(Error::NoBlocks);
        }
        Tipset::new(headers)
    }
    /// min_ticket returns the smallest ticket of all blocks in the tipset
    fn min_ticket(&self) -> Result<(Ticket), Error> {
        if self.blocks.is_empty() {
            return Err(Error::NoBlocks);
        }
        Ok(self.blocks[0].ticket.clone())
    }
    /// min_timestamp returns the smallest timestamp of all blocks in the tipset
    fn min_timestamp(&self) -> Result<u64, Error> {
        if self.blocks.is_empty() {
            return Err(Error::NoBlocks);
        }
        let mut min = self.blocks[0].timestamp;
        for i in 1..self.blocks.len() {
            if self.blocks[i].timestamp < min {
                min = self.blocks[i].timestamp
            }
        }
        Ok(min)
    }
    /// len returns the number of blocks in the tipset
    fn len(&self) -> usize {
        self.blocks.len()
    }
    /// is_empty returns true if no blocks present in tipset
    fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }
    /// key returns a key for the tipset.
    fn key(&self) -> TipSetKeys {
        self.key.clone()
    }
    /// height returns the block number of a tipset
    fn height(&self) -> u64 {
        self.blocks[0].height
    }
    /// parents returns the CIDs of the parents of the blocks in the tipset
    fn parents(&self) -> TipSetKeys {
        self.blocks[0].parents.clone()
    }
    /// weight returns the tipset's calculated weight
    fn weight(&self) -> u64 {
        self.blocks[0].weight
    }
}
