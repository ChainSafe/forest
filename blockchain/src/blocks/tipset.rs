#![allow(unused_variables)]
#![allow(dead_code)]

use super::block::BlockHeader;
use super::errors::Error;
use super::ticket::Ticket;
use cid::Cid;

/// TipSet is an immutable set of blocks at the same height with the same parent set
/// Blocks in a tipset are canonically ordered by ticket size
pub struct Tipset {
    blocks: Vec<BlockHeader>,
    key: TipSetKeys,
}

/// TipSetKeys is a set of CIDs forming a unique key for a TipSet
/// Equal keys will have equivalent iteration order, but note that the CIDs are *not* maintained in
/// the same order as the canonical iteration order of blocks in a tipset (which is by ticket)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TipSetKeys {
    pub cids: Vec<Cid>,
}

impl Tipset {
    /// new builds a new TipSet from a collection of blocks
    /// A valid tipset contains a non-empty collection of blocks that have distinct miners and all specify identical
    /// epoch, parents, weight, height, state root, receipt root;
    /// contentID for headers are supposed to be distinct but until encoding is added will be equal
    pub fn new(headers: Vec<BlockHeader>) -> Result<Self, Error> {
        // check header is non-empty
        if headers.is_empty() {
            return Err(Error::NoBlocks);
        }

        let mut sorted_headers = Vec::new();
        let mut sorted_cids = Vec::new();
        let mut i = 0;
        let size = headers.len() - 1;
        // loop through headers and validate conditions against 0th header
        while i <= size {
            if i > 0 {
                // skip redundant checks for first block
                // check height is equal
                if headers[i].height != headers[0].height {
                    return Err(Error::UndefinedTipSet);
                }
                // check parent cids are equal
                if !headers[i].parents.equals(headers[0].parents.clone()) {
                    println!("FAILS HERE::");
                    return Err(Error::UndefinedTipSet);
                }
                // check weights are equal
                if headers[i].weight != headers[0].weight {
                    return Err(Error::UndefinedTipSet);
                }
                // check state_roots are equal
                if headers[i].state_root != headers[0].state_root.clone() {
                    return Err(Error::UndefinedTipSet);
                }
                // check epochs are equal
                if headers[i].epoch != headers[0].epoch {
                    return Err(Error::UndefinedTipSet);
                }
                // check message_receipts are equal
                if headers[i].message_receipts != headers[0].message_receipts.clone() {
                    return Err(Error::UndefinedTipSet);
                }
                // check miner_addresses are distinct
                if headers[i].miner_address == headers[0].miner_address.clone() {
                    return Err(Error::UndefinedTipSet);
                }
            }
            // push headers into vec for sorting
            sorted_headers.push(headers[i].clone());
            // push header cid into vec for unqiue check
            sorted_cids.push(headers[i].clone().cid());
            i += 1;
        }

        // sort headers by ticket size
        // break ticket ties with the header CIDs, which are distinct
        sorted_headers.sort_by(|a, b| {
            let a1 = a.clone();
            let b1 = b.clone();

            a1.ticket
                .sort_key()
                .cmp(&b1.ticket.sort_key())
                .reverse()
                .then(a1.cid().hash.cmp(&b1.cid().hash))
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
                cids: sorted_cids,
            },
        })
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

impl TipSetKeys {
    /// equals checks whether the set contains exactly the same CIDs as another.
    fn equals(&self, key: TipSetKeys) -> bool {
        if self.cids.len() != key.cids.len() {
            return false;
        }
        let mut i = 0;
        let size = key.cids.len() - 1;
        while i <= size {
            if self.cids[i] != key.cids[i] {
                return false;
            }
            i += 1;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use address::Address;
    use cid::{Cid, Codec, Version};

    const WEIGHT: u64 = 0;
    const EPOCH: u64 = 1;
    const HEIGHT: u64 = 1;
    const CACHED_BYTES: u8 = 0;

    // key_setup returns a vec of 3 distinct CIDs
    fn key_setup() -> Vec<Cid> {
        let data0 = b"awesome test content!";
        let data1 = b"awesome test content am I right?";
        let data2 = b"awesome test content but seriously right?";
        let data3 = b"awesome test content for parents?";

        let h = multihash::encode(multihash::Hash::SHA2256, data0).unwrap();
        let cid = Cid::new(Codec::DagProtobuf, Version::V1, &h);
        let prefix = cid.prefix();

        let cid2 = Cid::new_from_prefix(&prefix, data1);
        let cid3 = Cid::new_from_prefix(&prefix, data2);
        // parents needs its own unique CID
        let cid4 = Cid::new_from_prefix(&prefix, data3);

        return vec![cid.clone(), cid2.clone(), cid3.clone(), cid4.clone()];
    }

    // template_header defines a block header used in testing
    fn template_header(ticket_p: Vec<u8>, cid: Cid, timestamp: u64) -> BlockHeader {
        let cids = key_setup();
        BlockHeader {
            parents: TipSetKeys {
                cids: vec![cids[3].clone()],
            },
            weight: WEIGHT,
            epoch: EPOCH,
            height: HEIGHT,
            miner_address: Address::new_secp256k1(ticket_p.clone()).unwrap(),
            messages: cids[0].clone(),
            message_receipts: cids[0].clone(),
            state_root: cids[0].clone(),
            timestamp,
            ticket: Ticket { vrfproof: ticket_p },
            election_proof: vec![],
            cached_cid: cid,
            cached_bytes: 0,
        }
    }

    // header_setup returns a vec of block headers to be used for testing purposes
    fn header_setup() -> Vec<BlockHeader> {
        let data0: Vec<u8> = vec![1, 4, 3, 6, 7, 1, 2];
        let data1: Vec<u8> = vec![1, 4, 3, 6, 1, 1, 2, 2, 4, 5, 3, 12, 2, 1];
        let data2: Vec<u8> = vec![1, 4, 3, 6, 1, 1, 2, 2, 4, 5, 3, 12, 2];
        let cids = key_setup();
        return vec![
            template_header(data1.clone(), cids[1].clone(), 1),
            template_header(data0.clone(), cids[0].clone(), 2),
            template_header(data2.clone(), cids[2].clone(), 3),
        ];
    }

    fn setup() -> Result<(Tipset), Error> {
        let headers = header_setup();
        let tipset = Tipset::new(headers.clone())?;
        Ok(tipset)
    }

    #[test]
    fn new_test() {
        let headers = header_setup();
        assert!(Tipset::new(headers).is_ok(), "result is okay!");
    }

    #[test]
    fn min_ticket_test() {
        let tipset = setup().unwrap();
        let min = Tipset::min_ticket(&tipset).unwrap();
        assert_eq!(min.vrfproof, tipset.blocks[0].ticket.vrfproof);
    }

    #[test]
    fn min_timestamp_test() {
        let tipset = setup().unwrap();
        let min_time = Tipset::min_timestamp(&tipset).unwrap();
        assert_eq!(min_time, tipset.blocks[1].timestamp);
    }

    #[test]
    fn len_test() {
        let tipset = setup().unwrap();
        assert_eq!(Tipset::len(&tipset), 3);
    }

    #[test]
    fn is_empty_test() {
        let tipset = setup().unwrap();
        assert_eq!(Tipset::is_empty(&tipset), false);
    }

    #[test]
    fn height_test() {
        let tipset = setup().unwrap();
        assert_eq!(Tipset::height(&tipset), tipset.blocks[1].height);
    }

    #[test]
    fn parents_test() {
        let tipset = setup().unwrap();
        assert_eq!(Tipset::parents(&tipset), tipset.blocks[1].parents);
    }

    #[test]
    fn weight_test() {
        let tipset = setup().unwrap();
        assert_eq!(Tipset::weight(&tipset), tipset.blocks[1].weight);
    }

    #[test]
    fn equals_test() {
        let tipset_keys = TipSetKeys {
            cids: key_setup().clone(),
        };
        assert_eq!(TipSetKeys::equals(&tipset_keys, tipset_keys.clone()), true);
    }
}
