extern crate cid;
use cid::Cid;

use crate::tipset::Tipset;
use std::time::SystemTime;
use vm::Address;

#[allow(dead_code)]
pub type BlockCID = Cid;
#[allow(dead_code)]
type MessageRoot = Cid;
#[allow(dead_code)]
type ReceiptRoot = Cid;

// should probably be bigint
#[allow(dead_code)]
type UVarint = u64;
#[allow(dead_code)]
pub type ChainWeight = UVarint;
#[allow(dead_code)]
pub type ChainEpoch = UVarint;
#[allow(dead_code)]
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
