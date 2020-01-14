// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

#![allow(unused_variables)]
#![allow(dead_code)]

use super::block::BlockHeader;
use super::errors::Error;
use super::ticket::Ticket;
use cid::Cid;
use clock::ChainEpoch;

/// A set of CIDs forming a unique key for a TipSet.
/// Equal keys will have equivalent iteration order, but note that the CIDs are *not* maintained in
/// the same order as the canonical iteration order of blocks in a tipset (which is by ticket)
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct TipSetKeys {
    pub cids: Vec<Cid>,
}

impl TipSetKeys {
    /// checks whether the set contains exactly the same CIDs as another.
    fn equals(&self, key: TipSetKeys) -> bool {
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
}

/// An immutable set of blocks at the same height with the same parent set.
/// Blocks in a tipset are canonically ordered by ticket size.
#[derive(Clone, PartialEq, Debug)]
pub struct Tipset {
    blocks: Vec<BlockHeader>,
    key: TipSetKeys,
}

impl Tipset {
    /// Returns all blocks in tipset
    pub fn blocks(&self) -> Vec<BlockHeader> {
        self.blocks.clone()
    }
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
                if !headers[i].parents.equals(headers[0].parents.clone()) {
                    return Err(Error::InvalidTipSet(
                        "parent cids are not equal".to_string(),
                    ));
                }
                // check weights are equal
                if headers[i].weight != headers[0].weight {
                    return Err(Error::InvalidTipSet("weights are not equal".to_string()));
                }
                // check state_roots are equal
                if headers[i].state_root != headers[0].state_root.clone() {
                    return Err(Error::InvalidTipSet(
                        "state_roots are not equal".to_string(),
                    ));
                }
                // check epochs are equal
                if headers[i].epoch != headers[0].epoch {
                    return Err(Error::InvalidTipSet("epochs are not equal".to_string()));
                }
                // check message_receipts are equal
                if headers[i].message_receipts != headers[0].message_receipts.clone() {
                    return Err(Error::InvalidTipSet(
                        "message_receipts are not equal".to_string(),
                    ));
                }
                // check miner_addresses are distinct
                if headers[i].miner_address == headers[0].miner_address.clone() {
                    return Err(Error::InvalidTipSet(
                        "miner_addresses are not distinct".to_string(),
                    ));
                }
            }
            // push headers into vec for sorting
            sorted_headers.push(headers[i].clone());
            // push header cid into vec for unique check
            cids.push(headers[i].clone().cid());
        }

        // sort headers by ticket size
        // break ticket ties with the header CIDs, which are distinct
        sorted_headers.sort_by_key(|header| {
            let mut h = header.clone();
            (h.ticket.vrfproof.clone(), h.cid().to_bytes())
        });

        // TODO
        // Have a check the ensures CIDs are distinct
        // blocked by CBOR encoding

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

    /// Returns the smallest ticket of all blocks in the tipset
    fn min_ticket(&self) -> Result<Ticket, Error> {
        if self.blocks.is_empty() {
            return Err(Error::NoBlocks);
        }
        Ok(self.blocks[0].ticket.clone())
    }
    /// Returns the smallest timestamp of all blocks in the tipset
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
    /// Returns the number of blocks in the tipset
    fn len(&self) -> usize {
        self.blocks.len()
    }
    /// Returns true if no blocks present in tipset
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }
    /// Returns a key for the tipset.
    pub fn key(&self) -> TipSetKeys {
        self.key.clone()
    }
    /// Returns the CIDs of the parents of the blocks in the tipset
    pub fn parents(&self) -> TipSetKeys {
        self.blocks[0].parents.clone()
    }
    /// Returns the tipset's calculated weight
    pub fn weight(&self) -> u64 {
        self.blocks[0].weight
    }
    /// Returns the tipset's epoch
    pub fn tip_epoch(&self) -> ChainEpoch {
        self.blocks[0].epoch.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::TxMeta;
    use address::Address;
    use cid::Cid;
    use clock::ChainEpoch;
    use crypto::VRFResult;

    const WEIGHT: u64 = 1;
    const CACHED_BYTES: [u8; 1] = [0];

    fn template_key(data: &[u8]) -> Cid {
        Cid::from_bytes_default(data).unwrap()
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
        BlockHeader {
            parents: TipSetKeys {
                cids: vec![cids[3].clone()],
            },
            weight: WEIGHT,
            epoch: ChainEpoch::new(1),
            miner_address: Address::new_secp256k1(ticket_p.clone()).unwrap(),
            messages: TxMeta {
                bls_messages: cids[0].clone(),
                secp_messages: cids[0].clone(),
            },
            message_receipts: cids[0].clone(),
            state_root: cids[0].clone(),
            timestamp,
            ticket: Ticket {
                vrfproof: VRFResult::new(ticket_p),
            },
            bls_aggregate: vec![1, 2, 3],
            cached_cid: cid,
            cached_bytes: CACHED_BYTES.to_vec(),
        }
    }

    // header_setup returns a vec of block headers to be used for testing purposes
    fn header_setup() -> Vec<BlockHeader> {
        let data0: Vec<u8> = vec![1, 4, 3, 6, 7, 1, 2];
        let data1: Vec<u8> = vec![1, 4, 3, 6, 1, 1, 2, 2, 4, 5, 3, 12, 2, 1];
        let data2: Vec<u8> = vec![1, 4, 3, 6, 1, 1, 2, 2, 4, 5, 3, 12, 2];
        let cids = key_setup();
        return vec![
            template_header(data0.clone(), cids[0].clone(), 1),
            template_header(data1.clone(), cids[1].clone(), 2),
            template_header(data2.clone(), cids[2].clone(), 3),
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
        let expected_value = vec![1, 4, 3, 6, 1, 1, 2, 2, 4, 5, 3, 12, 2];
        let min = Tipset::min_ticket(&tipset).unwrap();
        assert_eq!(min.vrfproof.to_bytes(), expected_value);
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
            Tipset::parents(&tipset),
            TipSetKeys {
                cids: vec!(expected_value)
            }
        );
    }

    #[test]
    fn weight_test() {
        let tipset = setup();
        assert_eq!(Tipset::weight(&tipset), 1);
    }

    #[test]
    fn equals_test() {
        let tipset_keys = TipSetKeys {
            cids: key_setup().clone(),
        };
        assert_eq!(TipSetKeys::equals(&tipset_keys, tipset_keys.clone()), true);
    }
}
