// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use super::ticket::Ticket;
use super::TipSetKeys;
use super::TxMeta;
use address::Address;
use cid::{Cid, Error as CidError};
use clock::ChainEpoch;
use crypto::Signature;
use derive_builder::Builder;
use encoding::Cbor;
use serde::{Deserialize, Serialize};

/// Header of a block
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
///     .message_receipts(Cid::default()) //optional
///     .state_root(Cid::default()) //optional
///     .timestamp(0) //optional
///     .ticket(Ticket::default()) //optional
///     .build()
///     .unwrap();
/// ```
#[derive(Clone, Debug, PartialEq, Builder, Serialize, Deserialize)]
#[builder(name = "BlockHeaderBuilder")]
pub struct BlockHeader {
    // CHAIN LINKING
    /// Parents is the set of parents this block was based on. Typically one,
    /// but can be several in the case where there were multiple winning ticket-
    /// holders for an epoch
    parents: TipSetKeys,

    /// weight is the aggregate chain weight of the parent set
    #[builder(default)]
    weight: u64,

    /// epoch is the period in which a new block is generated.
    /// There may be multiple rounds in an epoch
    #[builder(default)]
    epoch: ChainEpoch,

    // MINER INFO
    /// miner_address is the address of the miner actor that mined this block
    miner_address: Address,

    // STATE
    /// messages contains the merkle links for bls_messages and secp_messages
    #[builder(default)]
    messages: TxMeta,

    /// message_receipts is the Cid of the root of an array of MessageReceipts
    #[builder(default)]
    message_receipts: Cid,

    /// state_root is a cid pointer to the state tree after application of
    /// the transactions state transitions
    #[builder(default)]
    state_root: Cid,

    // CONSENSUS
    /// timestamp, in seconds since the Unix epoch, at which this block was created
    #[builder(default)]
    timestamp: u64,

    /// the ticket submitted with this block
    #[builder(default)]
    ticket: Ticket,

    // SIGNATURES
    /// aggregate signature of miner in block
    bls_aggregate: Signature,

    // CACHE
    /// stores the cid for the block after the first call to `cid()`
    #[builder(setter(skip))]
    #[serde(skip_serializing)]
    // TODO remove public visibility on cache values once tests reliance on them are removed
    pub cached_cid: Option<Cid>,
    /// stores the hashed bytes of the block after the fist call to `cid()`
    #[builder(setter(skip))]
    #[serde(skip_serializing)]
    pub cached_bytes: Option<Vec<u8>>,
}

impl Cbor for BlockHeader {}

impl BlockHeader {
    /// Generates a BlockHeader builder as a constructor
    pub fn builder() -> BlockHeaderBuilder {
        BlockHeaderBuilder::default()
    }
    /// Getter for BlockHeader parents
    pub fn parents(&self) -> &TipSetKeys {
        &self.parents
    }
    /// Getter for BlockHeader weight
    pub fn weight(&self) -> u64 {
        self.weight
    }
    /// Getter for BlockHeader epoch
    pub fn epoch(&self) -> &ChainEpoch {
        &self.epoch
    }
    /// Getter for BlockHeader miner_address
    pub fn miner_address(&self) -> &Address {
        &self.miner_address
    }
    /// Getter for BlockHeader messages
    pub fn messages(&self) -> &TxMeta {
        &self.messages
    }
    /// Getter for BlockHeader message_receipts
    pub fn message_receipts(&self) -> &Cid {
        &self.message_receipts
    }
    /// Getter for BlockHeader state_root
    pub fn state_root(&self) -> &Cid {
        &self.state_root
    }
    /// Getter for BlockHeader timestamp
    pub fn timestamp(&self) -> u64 {
        self.timestamp
    }
    /// Getter for BlockHeader ticket
    pub fn ticket(&self) -> &Ticket {
        &self.ticket
    }
    /// Getter for BlockHeader bls_aggregate
    pub fn bls_aggregate(&self) -> &Signature {
        &self.bls_aggregate
    }
    /// Getter for BlockHeader cid
    pub fn cid(&self) -> Result<Cid, CidError> {
        // TODO Encode blockheader using CBOR into cache_bytes
        // Currently content id for headers will be incomplete until encoding and supporting libraries are completed
        if let Some(cache_cid) = self.cached_cid.clone() {
            Ok(cache_cid)
        } else {
            Ok(Cid::from_bytes_default(&self.marshal_cbor()?)?)
        }
    }
    /// Updates cache and returns mutable reference of header back
    pub fn update_cache(&mut self) -> &mut Self {
        self.cached_bytes = self.marshal_cbor().ok();
        if let Some(bz) = &self.cached_bytes {
            self.cached_cid = Cid::from_bytes_default(&bz).ok();
        }
        self
    }
    /// Returns the cached id
    pub fn cached_cid(&self) -> Cid {
        if let Some(cid) = &self.cached_cid {
            cid.clone()
        } else {
            Cid::default()
        }
    }
}
