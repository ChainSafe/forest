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
};
use num_bigint::BigUint;
use serde::Deserialize;

/// A set of CIDs forming a unique key for a TipSet.
/// Equal keys will have equivalent iteration order, but note that the CIDs are *not* maintained in
/// the same order as the canonical iteration order of blocks in a tipset (which is by ticket)
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct TipSetKeys {
    pub cids: Vec<Cid>,
}

impl TipSetKeys {
    pub fn new(cids: Vec<Cid>) -> Self {
        Self { cids }
    }

    /// checks whether the set contains exactly the same CIDs as another.
    fn equals(&self, key: &TipSetKeys) -> bool {
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

impl ser::Serialize for TipSetKeys {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.cids.serialize(serializer)
    }
}

impl<'de> de::Deserialize<'de> for TipSetKeys {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let cids: Vec<Cid> = Deserialize::deserialize(deserializer)?;
        Ok(TipSetKeys { cids })
    }
}

/// An immutable set of blocks at the same height with the same parent set.
/// Blocks in a tipset are canonically ordered by ticket size.
#[derive(Clone, PartialEq, Debug)]
pub struct Tipset {
    blocks: Vec<BlockHeader>,
    key: TipSetKeys,
}

impl Tipset {
    /// Builds a new TipSet from a collection of blocks.
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
            key: TipSetKeys {
                // interim until CID check is in place
                cids,
            },
        })
    }
    /// Returns epoch of the tipset
    pub fn epoch(&self) -> &ChainEpoch {
        &self.blocks[0].epoch()
    }
    /// Returns all blocks in tipset
    pub fn blocks(&self) -> &[BlockHeader] {
        &self.blocks
    }
    /// Returns the smallest ticket of all blocks in the tipset
    fn min_ticket(&self) -> Result<Ticket, Error> {
        if self.blocks.is_empty() {
            return Err(Error::NoBlocks);
        }
        Ok(self.blocks[0].ticket().clone())
    }
    /// Returns the smallest timestamp of all blocks in the tipset
    pub fn min_timestamp(&self) -> Result<u64, Error> {
        if self.blocks.is_empty() {
            return Err(Error::NoBlocks);
        }
        let mut min = self.blocks[0].timestamp();
        for i in 1..self.blocks.len() {
            if self.blocks[i].timestamp() < min {
                min = self.blocks[i].timestamp()
            }
        }
        Ok(min)
    }
    /// Returns the number of blocks in the tipset
    fn len(&self) -> usize {
        self.blocks.len()
    }
    /// Returns true if no blocks present in tipset
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }
    /// Returns a key for the tipset.
    pub fn key(&self) -> &TipSetKeys {
        &self.key
    }
    /// Returns slice of Cids for the current tipset
    pub fn cids(&self) -> &[Cid] {
        &self.key.cids()
    }
    /// Returns the CIDs of the parents of the blocks in the tipset
    pub fn parents(&self) -> &TipSetKeys {
        &self.blocks[0].parents()
    }
    /// Returns the tipset's calculated weight
    pub fn weight(&self) -> &BigUint {
        &self.blocks[0].weight()
    }
}

/// FullTipSet is an expanded version of the TipSet that contains all the blocks and messages
#[derive(Debug, PartialEq, Clone)]
pub struct FullTipset {
    blocks: Vec<Block>,
}

impl FullTipset {
    /// constructor
    pub fn new(blks: Vec<Block>) -> Self {
        Self { blocks: blks }
    }
    /// Returns all blocks in a full tipset
    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }
    /// Returns a Tipset
    pub fn tipset(&self) -> Result<Tipset, Error> {
        let mut headers = Vec::new();

        for block in self.blocks() {
            headers.push(block.header().clone())
        }
        let tip: Tipset = Tipset::new(headers)?;
        Ok(tip)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use address::Address;
    use cid::{multihash::Hash::Blake2b256, Cid};
    use crypto::VRFResult;
    use num_bigint::BigUint;

    const WEIGHT: u64 = 1;
    const CACHED_BYTES: [u8; 1] = [0];

    fn template_key(data: &[u8]) -> Cid {
        Cid::from_bytes(data, Blake2b256).unwrap()
    }

    // key_setup returns a vec of 4 distinct CIDs
    fn key_setup() -> Vec<Cid> {
        return vec![
            template_key(b"test content"),
            template_key(b"awesome test content "),
            template_key(b"even better test content"),
            template_key(b"the best test content out there"),
        ];
    }

    // template_header defines a block header used in testing
    fn template_header(ticket_p: Vec<u8>, cid: Cid, timestamp: u64) -> BlockHeader {
        let cids = key_setup();
        let header = BlockHeader::builder()
            .parents(TipSetKeys {
                cids: vec![cids[3].clone()],
            })
            .miner_address(Address::new_secp256k1(&ticket_p).unwrap())
            .timestamp(timestamp)
            .ticket(Ticket {
                vrfproof: VRFResult::new(ticket_p),
            })
            .weight(BigUint::from(WEIGHT))
            .cached_cid(cid)
            .build()
            .unwrap();

        header
    }

    // header_setup returns a vec of block headers to be used for testing purposes
    fn header_setup() -> Vec<BlockHeader> {
        let data0: Vec<u8> = vec![1, 4, 3, 6, 7, 1, 2];
        let data1: Vec<u8> = vec![1, 4, 3, 6, 1, 1, 2, 2, 4, 5, 3, 12, 2, 1];
        let data2: Vec<u8> = vec![1, 4, 3, 6, 1, 1, 2, 2, 4, 5, 3, 12, 2];
        let cids = key_setup();
        return vec![
            template_header(data0, cids[0].clone(), 1),
            template_header(data1, cids[1].clone(), 2),
            template_header(data2, cids[2].clone(), 3),
        ];
    }

    fn setup() -> Tipset {
        let headers = header_setup();
        return Tipset::new(headers.clone()).expect("tipset is invalid");
    }

    #[test]
    fn new_test() {
        let headers = header_setup();
        assert!(Tipset::new(headers).is_ok(), "result is invalid");
    }

    #[test]
    fn min_ticket_test() {
        let tipset = setup();
        let expected_value: &[u8] = &[1, 4, 3, 6, 1, 1, 2, 2, 4, 5, 3, 12, 2];
        let min = Tipset::min_ticket(&tipset).unwrap();
        assert_eq!(min.vrfproof.bytes(), expected_value);
    }

    #[test]
    fn min_timestamp_test() {
        let tipset = setup();
        let min_time = Tipset::min_timestamp(&tipset).unwrap();
        assert_eq!(min_time, 1);
    }

    #[test]
    fn len_test() {
        let tipset = setup();
        assert_eq!(Tipset::len(&tipset), 3);
    }

    #[test]
    fn is_empty_test() {
        let tipset = setup();
        assert_eq!(Tipset::is_empty(&tipset), false);
    }

    #[test]
    fn parents_test() {
        let tipset = setup();
        let expected_value = template_key(b"the best test content out there");
        assert_eq!(
            *tipset.parents(),
            TipSetKeys {
                cids: vec!(expected_value)
            }
        );
    }

    #[test]
    fn weight_test() {
        let tipset = setup();
        assert_eq!(tipset.weight(), &BigUint::from(WEIGHT));
    }

    #[test]
    fn equals_test() {
        let tipset_keys = TipSetKeys {
            cids: key_setup().clone(),
        };
        let tipset_keys2 = TipSetKeys {
            cids: key_setup().clone(),
        };
        assert_eq!(tipset_keys.equals(&tipset_keys2), true);
    }
}
