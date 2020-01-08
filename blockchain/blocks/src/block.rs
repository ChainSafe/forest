// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

#![allow(dead_code)]

use super::ticket::Ticket;
use super::TipSetKeys;
use address::Address;
use cid::{Cid, Codec, Prefix, Version};
use clock::ChainEpoch;
use crypto::Signature;
use derive_builder::Builder;
use message::{SignedMessage, UnsignedMessage};
use multihash::Hash;

// DefaultHashFunction represents the default hashing function to use
// TODO SHOULD BE BLAKE2B
const DEFAULT_HASH_FUNCTION: Hash = Hash::Keccak256;
// TODO determine the purpose for these structures, currently spec includes them but with no definition
struct ChallengeTicketsCommitment {}
struct PoStCandidate {}
struct PoStRandomness {}
struct PoStProof {}

fn template_cid() -> Cid {
    Cid::new(Codec::DagCBOR, Version::V1, &[])
}

/// BlockHeader defines header of a block in the Filecoin blockchain
///
/// Usage:
/// ```
/// use blocks::{BlockHeader, TipSetKeys, Ticket, TxMeta};
/// use address::Address;
/// use cid::{Cid, Codec, Prefix, Version};
/// use clock::ChainEpoch;
///
/// BlockHeader::builder()
///     .parents(TipSetKeys::default())
///     .miner_address(Address::new_id(0).unwrap())
///     .bls_aggregate(vec![])
///     .weight(0) //optional
///     .epoch(ChainEpoch::default()) //optional
///     .messages(TxMeta::default()) //optional
///     .message_receipts(Cid::new(Codec::DagCBOR, Version::V1, &[])) //optional
///     .state_root(Cid::new(Codec::DagCBOR, Version::V1, &[])) //optional
///     .timestamp(0) //optional
///     .ticket(Ticket::default()) //optional
///     .build()
///     .unwrap();
/// ```
#[derive(Clone, Debug, PartialEq, Builder)]
#[builder(name = "BlockHeaderBuilder")]
pub struct BlockHeader {
    // CHAIN LINKING
    /// Parents is the set of parents this block was based on. Typically one,
    /// but can be several in the case where there were multiple winning ticket-
    /// holders for an epoch
    pub parents: TipSetKeys,

    /// weight is the aggregate chain weight of the parent set
    #[builder(default)]
    pub weight: u64,

    /// epoch is the period in which a new block is generated. There may be multiple rounds in an epoch
    #[builder(default)]
    pub epoch: ChainEpoch,

    // MINER INFO
    /// miner_address is the address of the miner actor that mined this block
    pub miner_address: Address,

    // STATE
    /// messages contains the merkle links for bls_messages and secp_messages
    #[builder(default)]
    pub messages: TxMeta,

    /// message_receipts is the Cid of the root of an array of MessageReceipts
    #[builder(default = "template_cid()")]
    pub message_receipts: Cid,

    /// state_root is a cid pointer to the state tree after application of the transactions state transitions
    #[builder(default = "template_cid()")]
    pub state_root: Cid,

    // CONSENSUS
    /// timestamp, in seconds since the Unix epoch, at which this block was created
    #[builder(default)]
    pub timestamp: u64,

    /// ticket is the ticket submitted with this block
    #[builder(default)]
    pub ticket: Ticket,

    // SIGNATURES
    /// aggregate signature of miner in block
    pub bls_aggregate: Signature,

    // CACHE
    #[builder(default = "template_cid()")]
    pub cached_cid: Cid,

    #[builder(default)]
    pub cached_bytes: Vec<u8>,
}

impl BlockHeader {
    pub fn builder() -> BlockHeaderBuilder {
        BlockHeaderBuilder::default()
    }
}

/// Block defines a full block
pub struct Block {
    header: BlockHeader,
    // TODO will rename to UnSignedMessage once changes are in
    bls_messages: UnsignedMessage,
    secp_messages: SignedMessage,
}

/// TxMeta tracks the merkleroots of both secp and bls messages separately
#[derive(Clone, Debug, PartialEq)]
pub struct TxMeta {
    pub bls_messages: Cid,
    pub secp_messages: Cid,
}

impl Default for TxMeta {
    fn default() -> Self {
        Self {
            bls_messages: template_cid(),
            secp_messages: template_cid(),
        }
    }
}

/// ElectionPoStVerifyInfo seems to be connected to VRF
/// see https://github.com/filecoin-project/lotus/blob/master/chain/sync.go#L1099
struct ElectionPoStVerifyInfo {
    candidates: PoStCandidate,
    randomness: PoStRandomness,
    proof: PoStProof,
    messages: Vec<UnsignedMessage>,
}

impl BlockHeader {
    /// cid returns the content id of this header
    pub fn cid(&mut self) -> Cid {
        // TODO
        // Encode blockheader into cache_bytes
        // Change DEFAULT_HASH_FUNCTION to utilize blake2b
        //
        // Currently content id for headers will be incomplete until encoding and supporting libraries are completed
        let c = Prefix {
            version: Version::V1,
            codec: Codec::DagCBOR,
            mh_type: DEFAULT_HASH_FUNCTION,
            mh_len: 8,
        };
        let new_cid = Cid::new_from_prefix(&c, &self.cached_bytes);
        self.cached_cid = new_cid;
        self.cached_cid.clone()
    }
}
