#![allow(dead_code)]

use super::ticket::{Ticket, VRFProofIndex};
use super::TipSetKeys;
extern crate multihash;
use multihash::{encode, decode, Hash};
use cid::{Cid, Version, Codec, Error, Prefix};
use vm::address::Address;

// DefaultHashFunction represents the default hashing function to use
const DEFAULT_HASH_FUNCTION: Hash = Hash::Blake2b;

/// BlockHeader defines header of a block in the Filecoin blockchain#[derive(Clone)]
#[derive(Clone)]
pub struct BlockHeader {
    /// CHAIN LINKING
    ///
    /// Parents is the set of parents this block was based on. Typically one,
    /// but can be several in the case where there were multiple winning ticket-
    /// holders for an epoch
    pub parents: TipSetKeys,
    /// weight is the aggregate chain weight of the parent set
    pub weight: u64,
    /// epoch is the period in which a new block is generated. There may be multiple rounds in an epoch
    epoch: u64,
    /// height is the block height
    pub height: u64,

    /// MINER INFO
    ///
    /// miner_address is the address of the miner actor that mined this block
    miner_address: Address,

    /// STATE
    ///
    /// messages is the Cid of the root of an array of Messages
    messages: Cid,
    /// message_receipts is the Cid of the root of an array of MessageReceipts
    message_receipts: Cid,
    /// state_root is a cid pointer to the state tree after application of the transactions state transitions
    state_root: Cid,

    /// CONSENSUS
    ///
    /// timestamp, in seconds since the Unix epoch, at which this block was created
    pub timestamp: u64,
    /// ticket is the ticket submitted with this block
    pub ticket: Ticket,
    /// election_proof is the "scratched ticket" proving that this block won
    /// an election
    election_proof: VRFProofIndex,
    // SIGNATURES
    //
    // block_sig filCrypto Signature
    // BLSAggregateSig
    
    /// CACHE
    /// 
    cachedCid: cid::Cid,

	cachedBytes: Vec<u8>,
}

/// Block defines a full block
pub struct Block {
    header: BlockHeader,
    // Messages
    // Receipts
}

impl Block {
    // cid returns the content id of this block
    fn cid(&self)  {
        
       let mut c = Prefix {
           version: Version::V0,
           codec: Codec::DagProtobuf,
           mh_type: DEFAULT_HASH_FUNCTION,
           mh_len: 0,
       };
       let res = c.as_bytes();
       let resp = Prefix::new_from_bytes(&res);
    }
}
