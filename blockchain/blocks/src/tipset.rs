// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![allow(unused_variables)]
#![allow(dead_code)]
use super::{Block, BlockHeader, Error, Ticket};
use cid::Cid;
use clock::ChainEpoch;
use encoding::{
    de::{self, Deserializer},
    ser::{self, Serializer},
    Cbor,
};
use num_bigint::BigUint;
use serde::Deserialize;

/// A set of CIDs forming a unique key for a Tipset.
/// Equal keys will have equivalent iteration order, but note that the CIDs are *not* maintained in
/// the same order as the canonical iteration order of blocks in a tipset (which is by ticket)
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct TipsetKeys {
    pub cids: Vec<Cid>,
}

impl TipsetKeys {
    pub fn new(cids: Vec<Cid>) -> Self {
        Self { cids }
    }

    /// Returns tipset header cids
    pub fn cids(&self) -> &[Cid] {
        &self.cids
    }
}

impl ser::Serialize for TipsetKeys {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.cids.serialize(serializer)
    }
}

impl<'de> de::Deserialize<'de> for TipsetKeys {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let cids: Vec<Cid> = Deserialize::deserialize(deserializer)?;
        Ok(TipsetKeys { cids })
    }
}

impl Cbor for TipsetKeys {}

/// An immutable set of blocks at the same height with the same parent set.
/// Blocks in a tipset are canonically ordered by ticket size.
#[derive(Clone, PartialEq, Debug, Eq)]
pub struct Tipset {
    blocks: Vec<BlockHeader>,
    key: TipsetKeys,
}

#[allow(clippy::len_without_is_empty)]
impl Tipset {
    /// Builds a new Tipset from a collection of blocks.
    /// A valid tipset contains a non-empty collection of blocks that have distinct miners and all
    /// specify identical epoch, parents, weight, height, state root, receipt root;
    /// contentID for headers are supposed to be distinct but until encoding is added will be equal.
    pub fn new(headers: Vec<BlockHeader>) -> Result<Self, Error> {
        verify_blocks(&headers)?;

        // TODO Have a check the ensures CIDs are distinct
        let cids = headers.iter().map(BlockHeader::cid).cloned().collect();

        // sort headers by ticket size
        // break ticket ties with the header CIDs, which are distinct
        let mut sorted_headers = headers;
        sorted_headers
            .sort_by_key(|header| (header.ticket().vrfproof.clone(), header.cid().to_bytes()));

        // return tipset where sorted headers have smallest ticket size in the 0th index
        // and the distinct keys
        Ok(Self {
            blocks: sorted_headers,
            key: TipsetKeys {
                // interim until CID check is in place
                cids,
            },
        })
    }
    /// Returns the first block of the tipset
    fn first_block(&self) -> &BlockHeader {
        // `Tipset::new` guarantees that `blocks` isn't empty
        self.blocks.first().unwrap()
    }
    /// Returns epoch of the tipset
    pub fn epoch(&self) -> ChainEpoch {
        self.first_block().epoch()
    }
    /// Returns all blocks in tipset
    pub fn blocks(&self) -> &[BlockHeader] {
        &self.blocks
    }
    /// Returns all blocks in tipset
    pub fn into_blocks(self) -> Vec<BlockHeader> {
        self.blocks
    }
    /// Returns the smallest ticket of all blocks in the tipset
    pub fn min_ticket(&self) -> Ticket {
        self.first_block().ticket().clone()
    }
    /// Returns the smallest timestamp of all blocks in the tipset
    pub fn min_timestamp(&self) -> u64 {
        self.blocks
            .iter()
            .map(|block| block.timestamp())
            .min()
            .unwrap()
    }
    /// Returns the number of blocks in the tipset
    pub fn len(&self) -> usize {
        self.blocks.len()
    }
    /// Returns a key for the tipset.
    pub fn key(&self) -> &TipsetKeys {
        &self.key
    }
    /// Returns slice of Cids for the current tipset
    pub fn cids(&self) -> &[Cid] {
        self.key.cids()
    }
    /// Returns the CIDs of the parents of the blocks in the tipset
    pub fn parents(&self) -> &TipsetKeys {
        self.first_block().parents()
    }
    /// Returns the state root for the tipset parent.
    pub fn parent_state(&self) -> &Cid {
        self.first_block().state_root()
    }
    /// Returns the tipset's calculated weight
    pub fn weight(&self) -> &BigUint {
        self.first_block().weight()
    }
}

/// FullTipset is an expanded version of the Tipset that contains all the blocks and messages
#[derive(Debug, PartialEq, Clone)]
pub struct FullTipset {
    blocks: Vec<Block>,
}

impl FullTipset {
    /// constructor
    pub fn new(blocks: Vec<Block>) -> Result<Self, Error> {
        verify_blocks(blocks.iter().map(Block::header))?;
        Ok(Self { blocks })
    }
    /// Returns the first block of the tipset
    fn first_block(&self) -> &Block {
        // `FullTipset::new` guarantees that `blocks` isn't empty
        self.blocks.first().unwrap()
    }
    /// Returns reference to all blocks in a full tipset
    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }
    /// Returns all blocks in a full tipset
    pub fn into_blocks(self) -> Vec<Block> {
        self.blocks
    }
    // TODO: conversions from full to regular tipset should not return a result
    // and should be validated on creation instead
    /// Returns a Tipset
    pub fn into_tipset(self) -> Result<Tipset, Error> {
        let headers = self.blocks.into_iter().map(|block| block.header).collect();
        Tipset::new(headers)
    }
    /// Returns a Tipset
    pub fn to_tipset(&self) -> Result<Tipset, Error> {
        let headers = self.blocks.iter().map(Block::header).cloned().collect();
        Tipset::new(headers)
    }
    /// Returns the state root for the tipset parent.
    pub fn parent_state(&self) -> &Cid {
        self.first_block().header().state_root()
    }
    /// Returns epoch of the tipset
    pub fn epoch(&self) -> ChainEpoch {
        self.first_block().header().epoch()
    }
    /// Returns the tipset's calculated weight
    pub fn weight(&self) -> &BigUint {
        self.first_block().header().weight()
    }
}

fn verify_blocks<'a, I>(headers: I) -> Result<(), Error>
where
    I: IntoIterator<Item = &'a BlockHeader>,
{
    let mut headers = headers.into_iter();
    let first_header = headers.next().ok_or(Error::NoBlocks)?;

    let verify = |predicate: bool, message: &'static str| {
        if predicate {
            Ok(())
        } else {
            Err(Error::InvalidTipset(message.to_string()))
        }
    };

    for header in headers {
        verify(
            header.parents() == first_header.parents(),
            "parent cids are not equal",
        )?;
        verify(
            header.weight() == first_header.weight(),
            "weights are not equal",
        )?;
        verify(
            header.state_root() == first_header.state_root(),
            "state_roots are not equal",
        )?;
        verify(
            header.epoch() == first_header.epoch(),
            "epochs are not equal",
        )?;
        verify(
            header.message_receipts() == first_header.message_receipts(),
            "message_receipts are not equal",
        )?;
        verify(
            header.miner_address() != first_header.miner_address(),
            "miner_addresses are not distinct",
        )?;
    }

    Ok(())
}
