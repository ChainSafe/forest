// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{EPostProof, Error, Ticket, TipSetKeys, Tipset};
use address::Address;
use cid::{multihash::Blake2b256, Cid};
use clock::ChainEpoch;
use crypto::Signature;
use derive_builder::Builder;
use encoding::{
    de::{self, Deserializer},
    ser::{self, Serializer},
    Cbor, Error as EncodingError,
};
use num_bigint::{
    biguint_ser::{BigUintDe, BigUintSer},
    BigUint,
};
use serde::Deserialize;
use sha2::Digest;
use std::cmp::Ordering;
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};
// TODO should probably have a central place for constants
const SHA_256_BITS: usize = 256;
const BLOCKS_PER_EPOCH: u64 = 5;

/// Header of a block
///
/// Usage:
/// ```
/// use forest_blocks::{BlockHeader, TipSetKeys, Ticket};
/// use address::Address;
/// use cid::{Cid, multihash::Identity};
/// use num_bigint::BigUint;
/// use crypto::Signature;
///
/// BlockHeader::builder()
///     .messages(Cid::new_from_cbor(&[], Identity)) // required
///     .message_receipts(Cid::new_from_cbor(&[], Identity)) // required
///     .state_root(Cid::new_from_cbor(&[], Identity)) // required
///     .miner_address(Address::new_id(0).unwrap()) // optional
///     .bls_aggregate(None) // optional
///     .parents(TipSetKeys::default()) // optional
///     .weight(BigUint::from(0u8)) // optional
///     .epoch(0) // optional
///     .timestamp(0) // optional
///     .ticket(Ticket::default()) // optional
///     .build_and_validate()
///     .unwrap();
/// ```
#[derive(Clone, Debug, PartialEq, Builder, Default, Eq)]
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
    #[builder(default)]
    messages: Cid,

    /// message_receipts is the Cid of the root of an array of MessageReceipts
    #[builder(default)]
    message_receipts: Cid,

    /// state_root is a cid pointer to the parent state root after calculating parent tipset.
    #[builder(default)]
    state_root: Cid,

    #[builder(default)]
    fork_signal: u64,

    #[builder(default)]
    signature: Option<Signature>,

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
    bls_aggregate: Option<Signature>,
    // CACHE
    /// stores the cid for the block after the first call to `cid()`
    #[builder(default)]
    cached_cid: Cid,

    /// stores the hashed bytes of the block after the fist call to `cid()`
    #[builder(default)]
    cached_bytes: Vec<u8>,
}

impl Cbor for BlockHeader {
    fn cid(&self) -> Result<Cid, EncodingError> {
        Ok(self.cid().clone())
    }
}

impl ser::Serialize for BlockHeader {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (
            &self.miner_address,
            &self.ticket,
            &self.epost_verify,
            &self.parents,
            BigUintSer(&self.weight),
            &self.epoch,
            &self.state_root,
            &self.message_receipts,
            &self.messages,
            &self.bls_aggregate,
            &self.timestamp,
            &self.signature,
            &self.fork_signal,
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
            BigUintDe(weight),
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

impl PartialOrd for BlockHeader {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.cached_bytes.partial_cmp(&other.cached_bytes)
    }
}

impl Ord for BlockHeader {
    fn cmp(&self, other: &Self) -> Ordering {
        self.cached_bytes.cmp(&other.cached_bytes)
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
    pub fn epoch(&self) -> ChainEpoch {
        self.epoch
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
    pub fn bls_aggregate(&self) -> &Option<Signature> {
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
    pub fn signature(&self) -> &Option<Signature> {
        &self.signature
    }
    /// Updates cache and returns mutable reference of header back
    fn update_cache(&mut self) -> Result<(), String> {
        self.cached_bytes = self.marshal_cbor().map_err(|e| e.to_string())?;
        self.cached_cid = Cid::new_from_cbor(&self.cached_bytes, Blake2b256);
        Ok(())
    }
    /// Check to ensure block signature is valid
    pub fn check_block_signature(&self, addr: &Address) -> Result<(), Error> {
        let signature = self
            .signature()
            .as_ref()
            .ok_or_else(|| Error::InvalidSignature("Signature is nil in header".to_owned()))?;

        signature
            .verify(&self.cid().to_bytes(), addr)
            .map_err(|e| Error::InvalidSignature(format!("Block signature invalid: {}", e)))?;

        Ok(())
    }
    /// Validates timestamps to ensure BlockHeader was generated at the correct time
    pub fn validate_timestamps(&self, base_tipset: &Tipset) -> Result<(), Error> {
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
            < base_tipset.min_timestamp()?
                + FIXED_BLOCK_DELAY * (self.epoch() - base_tipset.epoch())
        {
            return Err(Error::Validation(
                "Header was generated too soon".to_string(),
            ));
        }

        Ok(())
    }
    /// Returns true if (h(vrfout) * totalPower) < (e * sectorSize * 2^256)
    pub fn is_ticket_winner(&self, mpow: BigUint, net_pow: BigUint) -> bool {
        /*
        Need to check that
        (h(vrfout) + 1) / (max(h) + 1) <= e * myPower / totalPower
        max(h) == 2^256-1
        which in terms of integer math means:
        (h(vrfout) + 1) * totalPower <= e * myPower * 2^256
        in 2^256 space, it is equivalent to:
        h(vrfout) * totalPower < e * myPower * 2^256
        */

        // TODO switch ticket for election_proof
        let h = sha2::Sha256::digest(self.ticket.vrfproof.bytes());
        let mut lhs = BigUint::from_bytes_le(&h);
        lhs *= net_pow;

        // rhs = sectorSize * 2^256
        // rhs = sectorSize << 256
        let mut rhs = mpow << SHA_256_BITS;
        rhs *= BigUint::from(BLOCKS_PER_EPOCH);

        // h(vrfout) * totalPower < e * sectorSize * 2^256
        lhs < rhs
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
    use crate::BlockHeader;
    use encoding::Cbor;

    #[test]
    fn symmetric_header_encoding() {
        // This test vector is the genesis header for testnet/3 config
        let bz = hex::decode("8D4200008158207672662070726F6F66303030303030307672662070726F6F66303030303030308380581C692067756573732074686973206973206B696E64612072616E646F6D80804000D82A5827000171A0E420205B05D256933F3E29756306949643F34A0B644E475BF2BB9DAA81507C31B048A2D82A5827000171A0E4022001CD927FDCCD7938FABA323E32E70C44541B8A83F5DC941D90866565EF5AF14AD82A5827000171A0E402208D6F0E09E0453685B8816895CD56A7EE2FCE600026EE23AC445D78F020C1CA40F61A5E8BE968F600").unwrap();
        let header = BlockHeader::unmarshal_cbor(&bz).unwrap();
        assert_eq!(hex::encode(header.marshal_cbor().unwrap()), hex::encode(bz));
    }
}
