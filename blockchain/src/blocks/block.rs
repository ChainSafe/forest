#![allow(dead_code)]

extern crate cid;
use super::ticket::{Ticket, VRFPi};
use super::TipSetKey;
use cid::Cid;

type Address = String;

pub struct BlockHeader {
    // CHAIN LINKING
    //
    // Parents is the set of parents this block was based on. Typically one,
    // but can be several in the case where there were multiple winning ticket-
    // holders for an epoch
    pub parents: TipSetKey,
    // weight is the aggregate chain weight of the parent set
    pub weight: u64,
    //epoch is the period in which a new block is generated. There may be multiple rounds in an epoch
    pub epoch: u64,
    // height is the block height
    pub height: u64,

    // MINER INFO
    //
    // miner_address is the address of the miner actor that mined this block
    pub miner_address: Address,

    // STATE
    //
    // messages is the Cid of the root of an array of Messages
    pub messages: Cid,
    // message_receipts is the Cid of the root of an array of MessageReceipts
    pub message_receipts: Cid,
    // state_root is a cid pointer to the state tree after application of the transactions state transitions
    pub state_root: Cid,

    // CONSENSUS
    //
    // timestamp, in seconds since the Unix epoch, at which this block was created
    pub timestamp: u64,
    // ticket is the ticket submitted with this block
    pub ticket: Ticket,
    // election_proof is the "scratched ticket" proving that this block won
    // an election
    pub election_proof: VRFPi,
    // SIGNATURES
    //
    // block_sig filCrypto Signature
    // BLSAggregateSig
}

pub struct Block {
    header: BlockHeader,
    // Messages
    // Receipts
}
