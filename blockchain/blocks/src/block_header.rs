extern crate cid;
use cid::{Cid};

use message;
use crate::tipset::{Tipset};
use std::time::SystemTime;

type Address = String;
pub type BlockCID = Cid;

type MessageRoot = Cid;
type ReceiptRoot = Cid;

// should probably be bigint
type UVarint = u64;

pub type ChainWeight = UVarint;
pub type ChainEpoch = UVarint;

pub struct BlockHeader {
    parents: Tipset,
    weight: ChainWeight,
    epoch: ChainEpoch,
    miner_address: Address,
    //stateTree
    messages: MessageRoot,
    message_receipts: ReceiptRoot,
    timestamp: SystemTime,
    // ticket
    // election proof
    // blocksig filCrypto Signature
    // BLSAggregateSig 

}

