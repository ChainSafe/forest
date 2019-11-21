#![allow(unused_variables)]
#![allow(dead_code)]

extern crate cid;
use cid::Cid;

use super::block::BlockHeader;
use super::ticket::Ticket;

pub struct Tipset {
    blocks: Vec<BlockHeader>,
    key: TipSetKey,
}

// TipSetKey is a set of CIDs forming a unique key for a TipSet
// Equal keys will have equivalent iteration order, but note that the CIDs are *not* maintained in
// the same order as the canonical iteration order of blocks in a tipset (which is by ticket)
pub struct TipSetKey {
    pub cids: Vec<Cid>,
}

// new_tip_set builds a new TipSet from a collection of blocks
// The blocks must be distinct (different CIDs), have the same height, and same parent set
pub fn new_tip_set(_blocks: Vec<BlockHeader>) {
    // TODO
    // check length of blocks is not 0
    // loop through blocks to ensure blocks have same height and parent set
    // sort blocks by ticket size
    // check and assign uniqueness of key
    // return TipSet type
}

impl Tipset {
    // min_ticket returns the smallest ticket of all blocks in the tipset
    pub fn min_ticket(&self) -> Ticket {
        if self.blocks.is_empty() {
            return Ticket { vrfproof: vec![0] };
        }
        self.blocks[0].ticket.clone()
    }
    // min_timestamp returns the smallest timestamp of all blocks in the tipset
    pub fn min_timestamp(&self) -> u64 {
        if self.blocks.is_empty() {
            return 0;
        }
        let mut min = self.blocks[0].timestamp;
        for i in 1..self.blocks.len() {
            if self.blocks[i].timestamp < min {
                min = self.blocks[i].timestamp
            }
        }
        min
    }
    // len returns the number of blocks in the tipset
    pub fn len(&self) -> usize {
        self.blocks.len()
    }
    // is_empty returns true if no blocks present in tipset
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }
    // key returns a key for the tipset.
    pub fn key(&self) -> &TipSetKey {
        &self.key
    }
    // height returns the block number of a tipset
    pub fn height(&self) -> u64 {
        self.blocks[0].height
    }
    // parents returns the CIDs of the parents of the blocks in the tipset
    pub fn parents(&self) -> &TipSetKey {
        &self.blocks[0].parents
    }
    // weight returns the tipset's calculated weight
    pub fn weight(&self) -> u64 {
        self.blocks[0].weight
    }
}
