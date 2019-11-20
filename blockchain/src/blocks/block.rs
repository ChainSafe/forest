// SystemTime lib for timestamp
use std::time::{SystemTime};
// CID for external uuid
extern crate cid;
use cid::Cid;
// tipset module
use super::{TipSetKey};
use super::ticket::{Ticket, VRFPi};

type Address = String;

pub struct BlockHeader {
    // Chain linking
    //
    // Parents is the set of parents this block was based on. Typically one,
	// but can be several in the case where there were multiple winning ticket-
	// holders for an epoch.
    pub parents: TipSetKey,
    // weight is the aggregate chain weight of the parent set.
    pub weight: u64,
    //epoch is the period in which a new block is generated. There may be multiple rounds in an epoch
    pub epoch: u64,
    // height is the block height
    pub height: u64,
    // miner info
    //
    // miner_address is the address of the miner actor that mined this block.
    pub  miner_address: Address,
    
    // State
    //
    // messages is the set of messages included in this block. 
    // This field is the Cid of the root of an array of Messages.
    pub messages: Cid,
    // message_receipts is a set of receipts matching to the sending of the `Messages`.
    // This field is the Cid of the root of an array of MessageReceipts.
    pub message_receipts: Cid,
    // state_root is a cid pointer to the state tree after application of the transactions state transitions.
    pub state_root: Cid,

    // Consensus 
    //
    // timestamp, in seconds since the Unix epoch, at which this block was created.
    pub timestamp: SystemTime,
    // ticket is the ticket submitted with this block.
    pub ticket: Ticket,
    // election_proof is the "scratched ticket" proving that this block won
	// an election.
    pub election_proof: VRFPi,

    // Signatures
    //
    // blocksig filCrypto Signature
    // BLSAggregateSig
}
#[allow(dead_code)]
pub struct Block {
    header: BlockHeader,
    // Messages
    // Receipts
}
