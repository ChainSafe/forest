// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{EPostProof, Error, FullTipset, Ticket, TipSetKeys};
use address::Address;
use cid::{Cid, Error as CidError};
use clock::ChainEpoch;
use crypto::{is_valid_signature, Signature};
use derive_builder::Builder;
use encoding::{
    de::{self, Deserializer},
    ser::{self, Serializer},
    Cbor, Error as EncodingError,
};
use num_bigint::BigUint;
use raw_block::RawBlock;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

/// Header of a block
///
/// Usage:
/// ```
/// use forest_blocks::{BlockHeader, TipSetKeys, Ticket, TxMeta};
/// use address::Address;
/// use cid::{Cid, Codec, Prefix, Version};
/// use clock::ChainEpoch;
/// use num_bigint::BigUint;
/// use crypto::Signature;
///
/// BlockHeader::builder()
///     .miner_address(Address::new_id(0).unwrap()) // optional
///     .bls_aggregate(Signature::new_bls(vec![])) // optional
///     .parents(TipSetKeys::default()) // optional
///     .weight(BigUint::from(0u8)) // optional
///     .epoch(ChainEpoch::default()) // optional
///     .messages(Cid::default()) // optional
///     .message_receipts(Cid::default()) // optional
///     .state_root(Cid::default()) // optional
///     .timestamp(0) // optional
///     .ticket(Ticket::default()) // optional
///     .build_and_validate()
///     .unwrap();
/// ```
#[derive(Clone, Debug, PartialEq, Builder)]
#[builder(name = "BlockHeaderBuilder")]
pub struct BlockHeader {
    // CHAIN LINKING
    /// Parents is the set of parents this block was based on. Typically one,
    /// but can be several in the case where there were multiple winning ticket-
    /// holders for an epoch
    #[builder(default)]
    parents: TipSetKeys,

    /// weight is the aggregate chain weight of the parent set
    #[builder(default)]
    weight: BigUint,

    /// epoch is the period in which a new block is generated.
    /// There may be multiple rounds in an epoch.
    #[builder(default)]
    epoch: ChainEpoch,
    // MINER INFO
    /// miner_address is the address of the miner actor that mined this block
    #[builder(default)]
    miner_address: Address,

    // STATE
    /// messages contains the Cid to the merkle links for bls_messages and secp_messages
    /// The spec shows that messages is a TxMeta, but Lotus has it as a Cid to a TxMeta.
    /// TODO: Need to figure out how to convert TxMeta to a Cid
    #[builder(default)]
    messages: Cid,

    /// message_receipts is the Cid of the root of an array of MessageReceipts
    #[builder(default)]
    message_receipts: Cid,

    /// state_root is a cid pointer to the state tree after application of
    /// the transactions state transitions
    #[builder(default)]
    state_root: Cid,

    #[builder(default)]
    fork_signal: u64,

    #[builder(default)]
    signature: Signature,

    #[builder(default)]
    epost_verify: EPostProof,

    // CONSENSUS
    /// timestamp, in seconds since the Unix epoch, at which this block was created
    #[builder(default)]
    timestamp: u64,
    /// the ticket submitted with this block
    #[builder(default)]
    ticket: Ticket,
    // SIGNATURES
    /// aggregate signature of miner in block
    #[builder(default)]
    bls_aggregate: Signature,
    // CACHE
    /// stores the cid for the block after the first call to `cid()`
    #[builder(default)]
    cached_cid: Cid,

    /// stores the hashed bytes of the block after the fist call to `cid()`
    #[builder(default)]
    cached_bytes: Vec<u8>,
}

// TODO verify format or implement custom serialize/deserialize function (if necessary):
// https://github.com/ChainSafe/forest/issues/143

impl Cbor for BlockHeader {}

#[derive(Serialize, Deserialize)]
struct CborBlockHeader(
    Address,    // miner_address
    Ticket,     // ticket
    EPostProof, // epost_verify
    TipSetKeys, // parents []cid
    BigUint,    // weight
    ChainEpoch, // epoch
    Cid,        // state_root
    Cid,        // message_receipts
    Cid,        // messages
    Signature,  // bls_aggregate
    u64,        // timestamp
    Signature,  // signature
    u64,        // fork_signal
);

impl ser::Serialize for BlockHeader {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        CborBlockHeader(
            self.miner_address.clone(),
            self.ticket.clone(),
            self.epost_verify.clone(),
            self.parents.clone(),
            self.weight.clone(),
            self.epoch,
            self.state_root.clone(),
            self.message_receipts.clone(),
            self.messages.clone(),
            self.bls_aggregate.clone(),
            self.timestamp,
            self.signature.clone(),
            self.fork_signal,
        )
        .serialize(serializer)
    }
}

impl<'de> de::Deserialize<'de> for BlockHeader {
    fn deserialize<D>(deserializer: D) -> Result<Self, <D as Deserializer<'de>>::Error>
    where
        D: Deserializer<'de>,
    {
        let (
            miner_address,
            ticket,
            epost_verify,
            parents,
            weight,
            epoch,
            state_root,
            message_receipts,
            messages,
            bls_aggregate,
            timestamp,
            signature,
            fork_signal,
        ) = Deserialize::deserialize(deserializer)?;

        let header = BlockHeader::builder()
            .parents(parents)
            .weight(weight)
            .epoch(epoch)
            .miner_address(miner_address)
            .messages(messages)
            .message_receipts(message_receipts)
            .state_root(state_root)
            .fork_signal(fork_signal)
            .signature(signature)
            .epost_verify(epost_verify)
            .timestamp(timestamp)
            .ticket(ticket)
            .bls_aggregate(bls_aggregate)
            .build_and_validate()
            .unwrap();

        Ok(header)
    }
}

impl RawBlock for BlockHeader {
    /// returns the block raw contents as a byte array
    fn raw_data(&self) -> Result<Vec<u8>, EncodingError> {
        // TODO should serialize block header using CBOR encoding
        self.marshal_cbor()
    }
    /// returns the content identifier of the block
    fn cid(&self) -> Result<Cid, CidError> {
        Ok(self.cid().clone())
    }
}

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
    pub fn weight(&self) -> &BigUint {
        &self.weight
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
    pub fn messages(&self) -> &Cid {
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
    pub fn cid(&self) -> &Cid {
        // Cache should be initialized, otherwise will return default Cid
        &self.cached_cid
    }
    /// Getter for BlockHeader fork_signal
    pub fn fork_signal(&self) -> u64 {
        self.fork_signal
    }
    /// Getter for BlockHeader epost_verify
    pub fn epost_verify(&self) -> &EPostProof {
        &self.epost_verify
    }
    /// Getter for BlockHeader signature
    pub fn signature(&self) -> &Signature {
        &self.signature
    }
    /// Updates cache and returns mutable reference of header back
    fn update_cache(&mut self) -> Result<(), String> {
        self.cached_bytes = self.marshal_cbor().map_err(|e| e.to_string())?;
        self.cached_cid = Cid::from_bytes_default(&self.cached_bytes).map_err(|e| e.to_string())?;
        Ok(())
    }
    /// Check to ensure block signature is valid
    pub fn check_block_signature(&self, addr: &Address) -> Result<(), Error> {
        if self.signature().bytes().is_empty() {
            return Err(Error::InvalidSignature(
                "Signature is nil in header".to_string(),
            ));
        }

        if !is_valid_signature(&self.cid().to_bytes(), addr, self.signature()) {
            return Err(Error::InvalidSignature(
                "Block signature is invalid".to_string(),
            ));
        }

        Ok(())
    }
    /// Validates timestamps to ensure BlockHeader was generated at the correct time
    pub fn validate_timestamps(&self, base_tipset: &FullTipset) -> Result<(), Error> {
        // first check that it is not in the future; see https://github.com/filecoin-project/specs/blob/6ab401c0b92efb6420c6e198ec387cf56dc86057/validation.md
        // allowing for some small grace period to deal with small asynchrony
        // using ALLOWABLE_CLOCK_DRIFT from Lotus; see https://github.com/filecoin-project/lotus/blob/master/build/params_shared.go#L34:7
        const ALLOWABLE_CLOCK_DRIFT: u64 = 1;
        let time_now = SystemTime::now().duration_since(UNIX_EPOCH)?;
        if self.timestamp() > time_now.as_secs() + ALLOWABLE_CLOCK_DRIFT
            || self.timestamp() > time_now.as_secs()
        {
            return Err(Error::Validation("Header was from the future".to_string()));
        }
        const FIXED_BLOCK_DELAY: u64 = 45;
        // check that it is appropriately delayed from its parents including null blocks
        if self.timestamp()
            < base_tipset.tipset()?.min_timestamp()?
                + FIXED_BLOCK_DELAY
                    * (*self.epoch() - *base_tipset.tipset()?.tip_epoch()).chain_epoch()
        {
            return Err(Error::Validation(
                "Header was generated too soon".to_string(),
            ));
        }

        Ok(())
    }
}

/// human-readable string representation of a block CID
impl fmt::Display for BlockHeader {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "BlockHeader: {:?}", self.cid())
    }
}

impl BlockHeaderBuilder {
    pub fn build_and_validate(&self) -> Result<BlockHeader, String> {
        // Convert header builder into header struct
        let mut header = self.build()?;

        // TODO add validation function

        // Fill header cache with raw bytes and cid
        header.update_cache()?;

        Ok(header)
    }
}

#[cfg(test)]
mod tests {
    use super::super::{EPostProof, Ticket, TipSetKeys};

    use crate::{BlockHeader, EPostTicket};
    use address::Address;
    use base64;
    use cid::Cid;
    use clock::ChainEpoch;
    use crypto::{Signature, VRFResult};
    use encoding::from_slice;
    use encoding::to_vec;
    use num_bigint::BigUint;
    use std::convert::TryFrom;

    // From Lotus
    const HEADER_BYTES: &[u8] = &[
        0x8d, 0x43, 0x00, 0xcb, 0x09, 0x81, 0x58, 0x60, 0x96, 0x64, 0x49, 0x2f, 0x30, 0xe9, 0xb9,
        0x50, 0x3b, 0x71, 0x41, 0x0b, 0x1d, 0x38, 0x2e, 0x2b, 0xd4, 0x85, 0x7f, 0xe2, 0x15, 0x39,
        0xac, 0x92, 0x1b, 0xcb, 0x7f, 0xd0, 0x86, 0xd5, 0x78, 0x71, 0xe6, 0xdd, 0x5c, 0x31, 0xcd,
        0x23, 0x61, 0x8b, 0x52, 0x52, 0xb6, 0x2c, 0x7b, 0x44, 0x4c, 0x3a, 0x02, 0x9b, 0xba, 0xad,
        0xc2, 0x50, 0x57, 0x56, 0x81, 0x06, 0x47, 0x77, 0xf6, 0x04, 0x06, 0xc4, 0xff, 0x00, 0x6f,
        0x38, 0xfc, 0x61, 0x71, 0xfe, 0x45, 0xd4, 0x83, 0xe5, 0x15, 0x79, 0xd0, 0xe2, 0x47, 0x8b,
        0x7e, 0x5f, 0xde, 0x2c, 0x51, 0xd2, 0xe8, 0x64, 0x63, 0xaf, 0x86, 0xd3, 0xcb, 0xd5, 0x83,
        0x58, 0xc0, 0xae, 0x7f, 0x39, 0xba, 0x2a, 0x1d, 0x0f, 0x6f, 0x71, 0xbe, 0x02, 0x2e, 0xbc,
        0xde, 0x7f, 0x83, 0x7e, 0xc8, 0x5e, 0x08, 0x4f, 0xb5, 0x5b, 0x65, 0xde, 0x58, 0xbd, 0xcb,
        0xe9, 0xcf, 0x1c, 0x2b, 0x9e, 0x01, 0x32, 0x35, 0xab, 0x5f, 0xe8, 0x3a, 0x7d, 0x05, 0x10,
        0x80, 0xd7, 0x45, 0x61, 0xcb, 0xa5, 0x9e, 0x02, 0xcc, 0x0a, 0x8e, 0x75, 0x08, 0x7d, 0xad,
        0xd1, 0xe2, 0x87, 0xe0, 0x48, 0xe4, 0x8b, 0x1d, 0x23, 0x56, 0x29, 0xc1, 0x5f, 0x94, 0x74,
        0xd0, 0xec, 0xa4, 0x95, 0x56, 0xfd, 0xc8, 0xa7, 0x54, 0x4f, 0x99, 0xa2, 0x23, 0xbc, 0xe9,
        0xaa, 0x77, 0xd2, 0x5e, 0xfb, 0x44, 0xb9, 0x2b, 0x13, 0x0c, 0x54, 0x67, 0x9b, 0xfc, 0x6a,
        0x9b, 0x12, 0x45, 0x48, 0xb3, 0xa1, 0x78, 0x75, 0x20, 0x5a, 0xc7, 0x80, 0xad, 0x3a, 0x82,
        0x4d, 0x70, 0x97, 0x92, 0xda, 0xc5, 0x8d, 0xa2, 0xfc, 0x24, 0x20, 0x06, 0x85, 0x88, 0x3f,
        0x1f, 0x68, 0xd8, 0x46, 0x0c, 0x05, 0xb3, 0x5f, 0x41, 0xcb, 0xbe, 0xa5, 0x1c, 0xc5, 0x9a,
        0x20, 0xe3, 0xcd, 0x3e, 0x81, 0x22, 0x16, 0x2b, 0x3d, 0xba, 0x3e, 0x82, 0x6e, 0xb0, 0x1c,
        0x58, 0x8d, 0x86, 0x9d, 0xc5, 0xbc, 0x0b, 0x92, 0x50, 0x7d, 0xbf, 0x37, 0xee, 0x4c, 0x29,
        0x9a, 0x3b, 0x12, 0x1e, 0xcb, 0xc2, 0x01, 0x8b, 0x73, 0x47, 0xcb, 0xe0, 0xc1, 0x08, 0x58,
        0x60, 0x85, 0xda, 0x1d, 0x70, 0x2c, 0xf9, 0x90, 0xb2, 0x58, 0x45, 0xbf, 0x4f, 0x4f, 0xb9,
        0xb8, 0xcf, 0xd9, 0x11, 0xbd, 0xcf, 0x61, 0xd3, 0x62, 0x8c, 0xc9, 0xef, 0x43, 0x3a, 0x49,
        0x67, 0x43, 0xcb, 0xf4, 0xe5, 0x7d, 0x9d, 0xb3, 0xda, 0xe0, 0x36, 0x17, 0x13, 0x57, 0xe7,
        0x7f, 0x71, 0x74, 0xbe, 0x02, 0xf0, 0x03, 0x1e, 0x97, 0xa9, 0x40, 0xe0, 0xcc, 0x57, 0xfe,
        0x84, 0xd6, 0x46, 0xd3, 0xf7, 0xd9, 0x1d, 0x16, 0xdd, 0x31, 0x30, 0xd5, 0x2c, 0x3b, 0xff,
        0x58, 0x6c, 0x7e, 0x2e, 0x8e, 0x27, 0xfb, 0xb1, 0x8d, 0x0f, 0xf2, 0x98, 0x11, 0x02, 0xe9,
        0xe5, 0x32, 0x03, 0xeb, 0xc7, 0xb4, 0xb1, 0x81, 0x83, 0x58, 0x20, 0x4c, 0x59, 0x62, 0x53,
        0xaf, 0xe9, 0x75, 0xb8, 0xd1, 0xca, 0x89, 0x9e, 0x8e, 0x55, 0xcc, 0x4b, 0xbe, 0xea, 0x8d,
        0x87, 0x4c, 0x0e, 0xdc, 0xb4, 0xee, 0xf8, 0xa0, 0xbd, 0x71, 0xff, 0xbe, 0x32, 0x19, 0x01,
        0x1c, 0x05, 0x82, 0xd8, 0x2a, 0x58, 0x27, 0x00, 0x01, 0x71, 0xa0, 0xe4, 0x02, 0x20, 0x8b,
        0x8d, 0x2c, 0xea, 0xe5, 0x6e, 0xc0, 0x55, 0x38, 0xca, 0xdb, 0xf6, 0x60, 0xe7, 0xf5, 0x54,
        0x80, 0xc6, 0x9f, 0xfd, 0xd4, 0xac, 0xc1, 0xfa, 0x13, 0x0f, 0x34, 0x96, 0xe1, 0x65, 0x76,
        0x6b, 0xd8, 0x2a, 0x58, 0x27, 0x00, 0x01, 0x71, 0xa0, 0xe4, 0x02, 0x20, 0x29, 0xe9, 0xd3,
        0x34, 0xca, 0x2d, 0x45, 0xbd, 0x06, 0x8b, 0x38, 0x79, 0x0c, 0x9c, 0x7d, 0x51, 0x43, 0xab,
        0x64, 0x0a, 0x41, 0x53, 0x9c, 0xc0, 0xf1, 0xcb, 0x2e, 0xb4, 0x8d, 0xfd, 0x66, 0xfe, 0x45,
        0x00, 0x05, 0x73, 0x41, 0x7b, 0x19, 0x1c, 0x25, 0xd8, 0x2a, 0x58, 0x27, 0x00, 0x01, 0x71,
        0xa0, 0xe4, 0x02, 0x20, 0xf3, 0xd5, 0x5d, 0x3e, 0xba, 0xdb, 0x83, 0x28, 0x4d, 0x2c, 0x3c,
        0x42, 0xe5, 0x80, 0x68, 0xc5, 0xca, 0x97, 0xed, 0x04, 0x9f, 0x7b, 0xc4, 0xe9, 0x73, 0x95,
        0x92, 0x01, 0x34, 0xa9, 0x8a, 0x2e, 0xd8, 0x2a, 0x58, 0x27, 0x00, 0x01, 0x71, 0xa0, 0xe4,
        0x02, 0x20, 0x80, 0x4f, 0x37, 0x38, 0x7d, 0x1c, 0x7b, 0x02, 0x0e, 0xab, 0x22, 0xfe, 0x03,
        0xe7, 0x77, 0xa4, 0xf0, 0x8b, 0xb0, 0x5d, 0xad, 0xc2, 0xa9, 0x7a, 0xeb, 0xa2, 0x47, 0xbe,
        0x8a, 0x09, 0x1d, 0xa6, 0xd8, 0x2a, 0x58, 0x27, 0x00, 0x01, 0x71, 0xa0, 0xe4, 0x02, 0x20,
        0x88, 0x49, 0x23, 0x33, 0xd4, 0x3d, 0xf9, 0xe4, 0x3b, 0xb6, 0x59, 0x95, 0xae, 0x68, 0x39,
        0x1a, 0x7c, 0xef, 0xe0, 0xb7, 0x03, 0xae, 0x28, 0xa2, 0xa0, 0x63, 0xb0, 0x17, 0x9a, 0x1a,
        0x19, 0x19, 0x58, 0x61, 0x02, 0x94, 0xb6, 0x6d, 0x31, 0x0b, 0x93, 0xdb, 0xba, 0x8d, 0x3c,
        0x2e, 0x1a, 0xe0, 0x02, 0x7c, 0x7e, 0x07, 0x69, 0x7c, 0xc6, 0x87, 0xd4, 0xa1, 0xa5, 0x9d,
        0xdb, 0x7e, 0x62, 0x9a, 0x50, 0xb0, 0x07, 0x43, 0x00, 0xad, 0x66, 0xe1, 0x56, 0x08, 0x96,
        0xb6, 0x3a, 0xc2, 0xd6, 0xd2, 0x01, 0xd4, 0x04, 0x01, 0xbd, 0x40, 0xb3, 0x04, 0x5a, 0x2d,
        0xd9, 0x6d, 0xf5, 0x87, 0xc9, 0x14, 0x28, 0x3f, 0xfa, 0x65, 0x55, 0x84, 0x55, 0x3a, 0xe3,
        0xc5, 0x2a, 0x9e, 0xf8, 0x51, 0x61, 0xe0, 0x6d, 0x05, 0x99, 0x44, 0x03, 0xe5, 0xcc, 0x11,
        0xa8, 0xa6, 0xd7, 0xcf, 0x3d, 0x3b, 0xc0, 0x9a, 0xef, 0x73, 0xd1, 0x1a, 0x5e, 0x22, 0x05,
        0x11, 0x58, 0x61, 0x02, 0xac, 0x68, 0xd6, 0xa6, 0xf1, 0x3d, 0x87, 0xd0, 0xce, 0x69, 0x30,
        0x18, 0x26, 0x34, 0x6d, 0x0e, 0x1c, 0xd3, 0x3b, 0xa5, 0x3e, 0xf8, 0xd7, 0x8f, 0xcb, 0xfe,
        0x31, 0x4a, 0x73, 0x7b, 0x6a, 0x25, 0x8d, 0xbd, 0x8a, 0x26, 0x7a, 0x6a, 0xed, 0xd7, 0xeb,
        0x0a, 0xb1, 0x2f, 0x21, 0x9d, 0x51, 0xe9, 0x05, 0x58, 0xe4, 0x58, 0xc9, 0x27, 0x64, 0xe2,
        0xd8, 0x8b, 0x99, 0x3a, 0xbb, 0x14, 0x20, 0xbe, 0x8e, 0x15, 0x1b, 0xd2, 0x07, 0x11, 0x82,
        0x43, 0xf8, 0x24, 0x80, 0xf5, 0xbd, 0xa2, 0x57, 0xf5, 0x7d, 0x56, 0x57, 0x8c, 0xa1, 0x01,
        0xec, 0x89, 0x89, 0xcc, 0x9a, 0x0c, 0x27, 0xdf, 0x76, 0xa7, 0x00,
    ];

    #[test]
    fn decode_blockheader() {
        let b_header = build_header();
        // Decode
        let header: BlockHeader = from_slice(&HEADER_BYTES).unwrap();
        assert_eq!(b_header, header);
    }

    #[test]
    fn encode_blockheader() {
        let header = build_header();
        // Encode
        let header_bytes = to_vec(&header).unwrap();

        assert_eq!(&header_bytes, &HEADER_BYTES)
    }

    fn build_header() -> BlockHeader {
        let vrf_result = VRFResult::new(base64::decode("lmRJLzDpuVA7cUELHTguK9SFf+IVOaySG8t/0IbVeHHm3VwxzSNhi1JStix7REw6Apu6rcJQV1aBBkd39gQGxP8Abzj8YXH+RdSD5RV50OJHi35f3ixR0uhkY6+G08vV").unwrap());
        let ticket = Ticket::new(vrf_result);

        let parents: Vec<cid::Cid> = vec![
            Cid::try_from(
                "BAFY2BZACECFY2LHK4VXMAVJYZLN7MYHH6VKIBRU77XKKZQP2CMHTJFXBMV3GW".to_owned(),
            )
            .unwrap(),
            Cid::try_from(
                "BAFY2BZACEAU6TUZUZIWULPIGRM4HSDE4PVIUHK3EBJAVHHGA6HFS5NEN7VTP4".to_owned(),
            )
            .unwrap(),
        ];

        let etik = EPostTicket {
            partial: base64::decode("TFliU6/pdbjRyomejlXMS77qjYdMDty07vigvXH/vjI=").unwrap(),
            sector_id: 284,
            challenge_index: 5,
        };

        let epost: EPostProof = EPostProof{
            proof: base64::decode("rn85uiodD29xvgIuvN5/g37IXghPtVtl3li9y+nPHCueATI1q1/oOn0FEIDXRWHLpZ4CzAqOdQh9rdHih+BI5IsdI1YpwV+UdNDspJVW/cinVE+ZoiO86ap30l77RLkrEwxUZ5v8apsSRUizoXh1IFrHgK06gk1wl5LaxY2i/CQgBoWIPx9o2EYMBbNfQcu+pRzFmiDjzT6BIhYrPbo+gm6wHFiNhp3FvAuSUH2/N+5MKZo7Eh7LwgGLc0fL4MEI").unwrap(),
            post_rand: base64::decode("hdodcCz5kLJYRb9PT7m4z9kRvc9h02KMye9DOklnQ8v05X2ds9rgNhcTV+d/cXS+AvADHpepQODMV/6E1kbT99kdFt0xMNUsO/9YbH4ujif7sY0P8pgRAunlMgPrx7Sx").unwrap(),
            candidates: vec![etik]
        };

        BlockHeader::builder()
            .miner_address(Address::new_id(1227).unwrap())
            .bls_aggregate(Signature::new_bls(base64::decode("lLZtMQuT27qNPC4a4AJ8fgdpfMaH1KGlndt+YppQsAdDAK1m4VYIlrY6wtbSAdQEAb1AswRaLdlt9YfJFCg/+mVVhFU648UqnvhRYeBtBZlEA+XMEaim1889O8Ca73PR").unwrap()))
            .parents(TipSetKeys{ cids: parents})
            .weight(BigUint::from(91439483u64))
            .epoch(ChainEpoch::new(7205).unwrap())
            .messages(Cid::try_from("BAFY2BZACECEESIZT2Q67TZB3WZMZLLTIHENHZ37AW4B24KFCUBR3AF42DIMRS".to_owned()).unwrap())
            .state_root(Cid::try_from("BAFY2BZACEDZ5KXJ6XLNYGKCNFQ6EFZMANDC4VF7NASPXXRHJOOKZEAJUVGFC4".to_owned()).unwrap())
            .message_receipts(Cid::try_from("BAFY2BZACECAE6NZYPUOHWAQOVMRP4A7HO6SPBC5QLWW4FKL25OREPPUKBEO2M".to_owned()).unwrap())
            .timestamp(1579287825)
            .ticket(ticket)
            .signature(Signature::new_bls(base64::decode("rGjWpvE9h9DOaTAYJjRtDhzTO6U++NePy/4xSnN7aiWNvYomemrt1+sKsS8hnVHpBVjkWMknZOLYi5k6uxQgvo4VG9IHEYJD+CSA9b2iV/V9VleMoQHsiYnMmgwn33an").unwrap()))
            .fork_signal(0)
            .epost_verify(epost)
            .build_and_validate()
            .unwrap()
    }
}
