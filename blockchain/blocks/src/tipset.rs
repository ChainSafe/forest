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
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default, Ord, PartialOrd)]
pub struct TipsetKeys {
    pub cids: Vec<Cid>,
}

impl TipsetKeys {
    pub fn new(cids: Vec<Cid>) -> Self {
        Self { cids }
    }

    /// checks whether the set contains exactly the same CIDs as another.
    pub fn equals(&self, key: &TipsetKeys) -> bool {
        if self.cids.len() != key.cids.len() {
            return false;
        }
        for i in 0..key.cids.len() {
            if self.cids[i] != key.cids[i] {
                return false;
            }
        }
        true
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
#[derive(Clone, PartialEq, Debug, PartialOrd, Ord, Eq)]
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
        // check header is non-empty
        if headers.is_empty() {
            return Err(Error::NoBlocks);
        }

        let mut sorted_headers = Vec::new();
        let mut cids = Vec::new();

        // loop through headers and validate conditions against 0th header
        for i in 0..headers.len() {
            if i > 0 {
                // Skip redundant check
                // check parent cids are equal
                if !headers[i].parents().equals(headers[0].parents()) {
                    return Err(Error::InvalidTipSet(
                        "parent cids are not equal".to_string(),
                    ));
                }
                // check weights are equal
                if headers[i].weight() != headers[0].weight() {
                    return Err(Error::InvalidTipSet("weights are not equal".to_string()));
                }
                // check state_roots are equal
                if headers[i].state_root() != headers[0].state_root() {
                    return Err(Error::InvalidTipSet(
                        "state_roots are not equal".to_string(),
                    ));
                }
                // check epochs are equal
                if headers[i].epoch() != headers[0].epoch() {
                    return Err(Error::InvalidTipSet("epochs are not equal".to_string()));
                }
                // check message_receipts are equal
                if headers[i].message_receipts() != headers[0].message_receipts() {
                    return Err(Error::InvalidTipSet(
                        "message_receipts are not equal".to_string(),
                    ));
                }
                // check miner_addresses are distinct
                if headers[i].miner_address() == headers[0].miner_address() {
                    return Err(Error::InvalidTipSet(
                        "miner_addresses are not distinct".to_string(),
                    ));
                }
            }
            // push headers into vec for sorting
            sorted_headers.push(headers[i].clone());
            // push header cid into vec for unique check (can be changed to hashset later)
            cids.push(headers[i].cid().clone());
        }

        // sort headers by ticket size
        // break ticket ties with the header CIDs, which are distinct
        sorted_headers
            .sort_by_key(|header| (header.ticket().vrfproof.clone(), header.cid().to_bytes()));

        // TODO Have a check the ensures CIDs are distinct

        // return tipset where sorted headers have smallest ticket size is in the 0th index
        // and the distinct keys
        Ok(Self {
            blocks: sorted_headers,
            key: TipsetKeys {
                // interim until CID check is in place
                cids,
            },
        })
    }
    /// Returns epoch of the tipset
    pub fn epoch(&self) -> ChainEpoch {
        self.blocks[0].epoch()
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
        self.blocks[0].ticket().clone()
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
        &self.key.cids()
    }
    /// Returns the CIDs of the parents of the blocks in the tipset
        &self.blocks[0].parents()
    pub fn parents(&self) -> &TipsetKeys {
    }
    /// Returns the state root for the tipset parent.
    pub fn parent_state(&self) -> &Cid {
        self.blocks[0].state_root()
    }
    /// Returns the tipset's calculated weight
    pub fn weight(&self) -> &BigUint {
        &self.blocks[0].weight()
    }
}

/// FullTipset is an expanded version of the Tipset that contains all the blocks and messages
#[derive(Debug, PartialEq, Clone)]
pub struct FullTipset {
    blocks: Vec<Block>,
}

impl FullTipset {
    /// constructor, panics when the given vector is empty
    pub fn new(blocks: Vec<Block>) -> Self {
        assert!(!blocks.is_empty());
        Self { blocks }
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
        let mut headers = Vec::new();

        for block in self.into_blocks() {
            headers.push(block.header)
        }
        let tip: Tipset = Tipset::new(headers)?;
        Ok(tip)
    }
    /// Returns a Tipset
    pub fn to_tipset(&self) -> Result<Tipset, Error> {
        let mut headers = Vec::new();

        for block in self.blocks() {
            headers.push(block.header().clone())
        }
        let tip: Tipset = Tipset::new(headers)?;
        Ok(tip)
    }
    /// Returns the state root for the tipset parent.
    pub fn parent_state(&self) -> &Cid {
        self.blocks[0].header().state_root()
    }
    /// Returns epoch of the tipset
    pub fn epoch(&self) -> ChainEpoch {
        self.blocks[0].header().epoch()
    }
    /// Returns the tipset's calculated weight
    pub fn weight(&self) -> &BigUint {
        &self.blocks[0].header().weight()
    }
}
