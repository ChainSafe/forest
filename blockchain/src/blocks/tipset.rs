// CID for external uuid
extern crate cid;
use cid::Cid;
use std::time::{SystemTime};
//pub use self::errors::Error;
use super::block::{BlockHeader};
use super::ticket::{Ticket};

pub struct Tipset {
    blocks: Vec<BlockHeader>,
    key: TipSetKey,
}

// TipSetKey is an immutable set of CIDs forming a unique key for a TipSet.
// Equal keys will have equivalent iteration order, but note that the CIDs are *not* maintained in
// the same order as the canonical iteration order of blocks in a tipset (which is by ticket).
// TipSetKey is a lightweight value type; passing by pointer is usually unnecessary.
pub struct TipSetKey {
	// The slice is wrapped in a struct to enforce immutability.
	cids: Vec<Cid>,
}

// new_tip_set builds a new TipSet from a collection of blocks.
// The blocks must be distinct (different CIDs), have the same height, and same parent set.
pub fn new_tip_set(_blocks: Vec<BlockHeader>) {
    // if blocks.len() == 0 {
    //    // return  Err(Error::UndefinedTipSet)
    // }
    let f = &_blocks[0];
    let height = f.height;
    //let parents = f.parents;
    let weight = f.weight;

    println!("{}, {}", height, weight);
}

pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

#[cfg(test)]
mod tests {
    use super::*;


    #[test]
    fn test_add() {
        assert_eq!(add(1, 2), 3);
    }
    
    #[test]
    fn test_new_tip_set() {
        let cid: Cid = "QmdfTbBqBPQ7VNxZEYEj14VmRuZBkqFbiwReogJgS1zR1n".parse().unwrap();
        let cid1: Cid = "QmdfTbBqBPQ7VNxZEYEj14VmRuZBkqFbiwReogJgS1zR11".parse().unwrap();
        let cid2: Cid = "QmdfTbBqBPQ7VNxZEYEj14VmRuZBkqFbiwReogJgS1zR12".parse().unwrap();
        let cid3: Cid = "QmdfTbBqBPQ7VNxZEYEj14VmRuZBkqFbiwReogJgS1zR13".parse().unwrap();

        let block = vec!(BlockHeader {
            parents: TipSetKey{
                cids: vec!(cid),
            },
            weight: 10,
            epoch: 10,
            height: 10,
            miner_address: "0x".to_string(),
            messages: cid1,
            message_receipts: cid2,
            state_root: cid3,
            timestamp: SystemTime::now(),
            ticket: Ticket{
                vrfproof: 0
            },
            election_proof: 0
        });

       new_tip_set(block)
    }
}

// trait Ts {
//     fn MinTicket() -> Ticket
// }