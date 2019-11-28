#![allow(unused_variables)]
#![allow(dead_code)]

use super::block::BlockHeader;
use super::errors::Error;
use super::ticket::Ticket;
use cid::{Cid, Codec, Version};

/// TipSet is an immutable set of blocks at the same height with the same parent set
/// Blocks in a tipset are canonically ordered by ticket size
pub struct Tipset {
    blocks: Vec<BlockHeader>,
    key: TipSetKeys,
}

/// TipSetKeys is a set of CIDs forming a unique key for a TipSet
/// Equal keys will have equivalent iteration order, but note that the CIDs are *not* maintained in
/// the same order as the canonical iteration order of blocks in a tipset (which is by ticket)
#[derive(PartialEq, Clone, Debug)]
pub struct TipSetKeys {
    pub cids: Vec<Cid>,
}

impl Tipset {
    /// new builds a new TipSet from a collection of blocks
    /// The blocks must be distinct (different CIDs), have the same height, and same parent set
    pub fn new(headers: Vec<BlockHeader>) -> Result<Self, Error> {
        if headers.is_empty() {
            return Err(Error::NoBlocks);
        }

        let mut sorted_headers = Vec::new();
        let mut sorted_cids = Vec::new();
        let mut i = 0;
        while i < headers.len() {
            if headers[i].height != headers[0].height {
                return Err(Error::UndefinedTipSet);
            }
            if !headers[i].parents.equals(headers[0].parents.clone()) {
                return Err(Error::UndefinedTipSet);
            }
            if headers[i].weight != headers[0].weight {
                return Err(Error::UndefinedTipSet);
            }
            if headers[i].state_root != headers[0].state_root.clone() {
                return Err(Error::UndefinedTipSet);
            }
            if headers[i].epoch != headers[0].epoch {
                return Err(Error::UndefinedTipSet);
            }
            if headers[i].message_receipts != headers[0].message_receipts.clone() {
                return Err(Error::UndefinedTipSet);
            }
            sorted_headers.push(headers[i].clone());
            sorted_cids.push(headers[i].clone().cid());
            i += 1;
        }
        // sort headers by ticket
        //
        // GO FILE COIN LOGIC BELOW
        //
        // sort.Slice(sorted, func(i, j int) bool {
        //     cmp := bytes.Compare(sorted[i].Ticket.SortKey(), sorted[j].Ticket.SortKey())
        //     if cmp == 0 {
        //         // Break ticket ties with the block CIDs, which are distinct.
        //         cmp = bytes.Compare(sorted[i].Cid().Bytes(), sorted[j].Cid().Bytes())
        //     }
        //     return cmp < 0
        // })
             

        println!("sorted headers -> {:?} \n \n", sorted_headers[0].ticket.sort_key());
        // sort headers by ticket size
        // if tie; Break ticket ties with the block CIDs, which are distinct
        let sorted = sorted_headers.sort_by(|a, b| {
            a.ticket
                .sort_key()
                .cmp(&b.ticket.sort_key())
        });
       
        println!("sorted headers after sort -> {:?}", sorted);

        // INTERIM TO SATISFY STRUCT
        let data = b"awesome test content";
        let h = multihash::encode(multihash::Hash::SHA2256, data).unwrap();

        let cid = Cid::new(Codec::DagProtobuf, Version::V1, &h);
        let prefix = cid.prefix();

        let cid2 = Cid::new_from_prefix(&prefix, data);

        Ok(Self {
            blocks: sorted_headers,
            key: TipSetKeys {
                cids: vec![cid.clone(), cid2.clone()],
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
        while i > key.cids.len() {
            i += 1;
            if self.cids[i] == key.cids[i] {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vm::address::Address;

    const WEIGHT: u64 = 0;
    const EPOCH: u64 = 1;
    const HEIGHT: u64 = 1;
    const CACHED_BYTES: u8 = 0;

    fn key_setup() -> Vec<Cid> {
        let data = b"awesome test content";
        let h = multihash::encode(multihash::Hash::SHA2256, data).unwrap();

        let cid = Cid::new(Codec::DagProtobuf, Version::V1, &h);
        let prefix = cid.prefix();

        let cid2 = Cid::new_from_prefix(&prefix, data);

        return vec![cid.clone(), cid2.clone()];
    }

    fn header_setup() -> Vec<BlockHeader> {
        let data: Vec<u8> = vec![1, 4, 3, 6, 7, 1, 2];
        let data2: Vec<u8> = vec![1, 4, 3, 6, 1, 1, 2, 2, 4, 5, 3];
        let new_addr = Address::new_secp256k1(data.clone()).unwrap();

        return vec![
            BlockHeader {
                parents: TipSetKeys {
                    cids: key_setup().clone(),
                },
                weight: WEIGHT,
                epoch: EPOCH,
                height: HEIGHT,
                miner_address: new_addr.clone(),
                messages: key_setup()[0].clone(),
                message_receipts: key_setup()[0].clone(),
                state_root: key_setup()[0].clone(),
                timestamp: 0,
                ticket: Ticket {
                    vrfproof: data2.clone(),
                },
                election_proof: data.clone(),
                cached_cid: key_setup()[0].clone(),
                cached_bytes: CACHED_BYTES,
            },
            BlockHeader {
                parents: TipSetKeys { cids: key_setup() },
                weight: WEIGHT,
                epoch: EPOCH,
                height: HEIGHT,
                miner_address: new_addr,
                messages: key_setup()[0].clone(),
                message_receipts: key_setup()[0].clone(),
                state_root: key_setup()[0].clone(),
                timestamp: 1,
                ticket: Ticket { vrfproof: data.clone() },
                election_proof: data.clone(),
                cached_cid: key_setup()[1].clone(),
                cached_bytes: CACHED_BYTES,
            },
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
    fn min_ticket_test() -> Result<(), Error> {
        let tipset = setup()?;
        let min = Tipset::min_ticket(&tipset)?;
        assert_eq!(min.vrfproof, tipset.blocks[0].ticket.vrfproof);
        Ok(())
    }

    #[test]
    fn min_timestamp_test() -> Result<(), Error> {
        let tipset = setup()?;
        let min_time = Tipset::min_timestamp(&tipset)?;
        assert_eq!(min_time, tipset.blocks[0].timestamp);
        Ok(())
    }

    #[test]
    fn len_test() -> Result<(), Error> {
        let tipset = setup()?;
        assert_eq!(Tipset::len(&tipset), 2);
        Ok(())
    }

    #[test]
    fn is_empty_test() -> Result<(), Error> {
        let tipset = setup()?;
        assert_eq!(Tipset::is_empty(&tipset), false);
        Ok(())
    }

    #[test]
    fn key_test() -> Result<(), Error> {
        let keys = TipSetKeys {
            cids: key_setup().clone(),
        };
        let headers = header_setup();
        let tipset = Tipset::new(headers.clone())?;
        assert_eq!(Tipset::key(&tipset), keys);
        Ok(())
    }

    #[test]
    fn height_test() -> Result<(), Error> {
        let tipset = setup()?;
        assert_eq!(Tipset::height(&tipset), tipset.blocks[1].height);
        Ok(())
    }

    #[test]
    fn parents_test() -> Result<(), Error> {
        let tipset = setup()?;
        assert_eq!(Tipset::parents(&tipset), tipset.blocks[1].parents);
        Ok(())
    }

    #[test]
    fn weight_test() -> Result<(), Error> {
        let tipset = setup()?;
        assert_eq!(Tipset::weight(&tipset), tipset.blocks[1].weight);
        Ok(())
    }

    #[test]
    fn equals_test() {
        let tipset_keys = TipSetKeys {
            cids: key_setup().clone(),
        };
        assert!(TipSetKeys::equals(&tipset_keys, tipset_keys.clone()));
    }
}
