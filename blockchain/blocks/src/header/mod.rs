// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{ElectionProof, Error, Ticket, TipsetKeys};
use address::Address;
use beacon::{self, Beacon, BeaconEntry};
use cid::{Cid, Code::Blake2b256};
use clock::ChainEpoch;
use crypto::Signature;
use derive_builder::Builder;
use encoding::blake2b_256;
use encoding::{Cbor, Error as EncodingError};
use fil_types::PoStProof;
use num_bigint::{
    bigint_ser::{BigIntDe, BigIntSer},
    BigInt,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::Digest;
use std::fmt;
use vm::TokenAmount;

#[cfg(feature = "json")]
pub mod json;

// TODO should probably have a central place for constants
const SHA_256_BITS: usize = 256;
const BLOCKS_PER_EPOCH: u64 = 5;

/// Header of a block
///
/// Usage:
/// ```
/// use forest_blocks::{BlockHeader, TipsetKeys, Ticket};
/// use address::Address;
/// use cid::{Cid, Code::Identity};
/// use num_bigint::BigInt;
/// use crypto::Signature;
///
/// BlockHeader::builder()
///     .messages(Cid::new_from_cbor(&[], Identity)) // required
///     .message_receipts(Cid::new_from_cbor(&[], Identity)) // required
///     .state_root(Cid::new_from_cbor(&[], Identity)) // required
///     .miner_address(Address::new_id(0)) // optional
///     .beacon_entries(Vec::new()) // optional
///     .winning_post_proof(Vec::new()) // optional
///     .election_proof(None) // optional
///     .bls_aggregate(None) // optional
///     .signature(None) // optional
///     .parents(TipsetKeys::default()) // optional
///     .weight(BigInt::from(0u8)) // optional
///     .epoch(0) // optional
///     .timestamp(0) // optional
///     .ticket(Some(Ticket::default())) // optional
///     .fork_signal(0) // optional
///     .build_and_validate()
///     .unwrap();
/// ```
#[derive(Clone, Debug, PartialEq, Builder, Eq)]
#[builder(name = "BlockHeaderBuilder")]
pub struct BlockHeader {
    // CHAIN LINKING
    /// Parents is the set of parents this block was based on. Typically one,
    /// but can be several in the case where there were multiple winning ticket-
    /// holders for an epoch
    #[builder(default)]
    parents: TipsetKeys,

    /// weight is the aggregate chain weight of the parent set
    #[builder(default)]
    weight: BigInt,

    /// epoch is the period in which a new block is generated.
    /// There may be multiple rounds in an epoch.
    #[builder(default)]
    epoch: ChainEpoch,

    /// BeaconEntries contain the verifiable oracle randomness used to elect
    /// this block's author leader
    #[builder(default)]
    beacon_entries: Vec<BeaconEntry>,

    /// PoStProofs are the winning post proofs
    #[builder(default)]
    winning_post_proof: Vec<PoStProof>,

    // MINER INFO
    /// miner_address is the address of the miner actor that mined this block
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
    election_proof: Option<ElectionProof>,

    // CONSENSUS
    /// timestamp, in seconds since the Unix epoch, at which this block was created
    #[builder(default)]
    timestamp: u64,
    /// the ticket submitted with this block
    #[builder(default)]
    ticket: Option<Ticket>,
    // SIGNATURES
    /// aggregate signature of miner in block
    #[builder(default)]
    bls_aggregate: Option<Signature>,
    /// the base fee of the parent block
    #[builder(default)]
    parent_base_fee: TokenAmount,
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
        Ok(*self.cid())
    }
}

impl Serialize for BlockHeader {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (
            &self.miner_address,
            &self.ticket,
            &self.election_proof,
            &self.beacon_entries,
            &self.winning_post_proof,
            &self.parents,
            BigIntSer(&self.weight),
            &self.epoch,
            &self.state_root,
            &self.message_receipts,
            &self.messages,
            &self.bls_aggregate,
            &self.timestamp,
            &self.signature,
            &self.fork_signal,
            BigIntSer(&self.parent_base_fee),
        )
            .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for BlockHeader {
    fn deserialize<D>(deserializer: D) -> Result<Self, <D as Deserializer<'de>>::Error>
    where
        D: Deserializer<'de>,
    {
        let (
            miner_address,
            ticket,
            election_proof,
            beacon_entries,
            winning_post_proof,
            parents,
            BigIntDe(weight),
            epoch,
            state_root,
            message_receipts,
            messages,
            bls_aggregate,
            timestamp,
            signature,
            fork_signal,
            BigIntDe(parent_base_fee),
        ) = Deserialize::deserialize(deserializer)?;

        let header = BlockHeader::builder()
            .parents(parents)
            .weight(weight)
            .epoch(epoch)
            .beacon_entries(beacon_entries)
            .winning_post_proof(winning_post_proof)
            .miner_address(miner_address)
            .messages(messages)
            .message_receipts(message_receipts)
            .state_root(state_root)
            .fork_signal(fork_signal)
            .signature(signature)
            .election_proof(election_proof)
            .timestamp(timestamp)
            .ticket(ticket)
            .bls_aggregate(bls_aggregate)
            .parent_base_fee(parent_base_fee)
            .build_and_validate()
            .unwrap();

        Ok(header)
    }
}

impl BlockHeader {
    /// Generates a BlockHeader builder as a constructor
    pub fn builder() -> BlockHeaderBuilder {
        BlockHeaderBuilder::default()
    }
    /// Getter for BlockHeader parents
    pub fn parents(&self) -> &TipsetKeys {
        &self.parents
    }
    /// Getter for BlockHeader weight
    pub fn weight(&self) -> &BigInt {
        &self.weight
    }
    /// Getter for BlockHeader epoch
    pub fn epoch(&self) -> ChainEpoch {
        self.epoch
    }
    /// Getter for Drand BeaconEntry
    pub fn beacon_entries(&self) -> &[BeaconEntry] {
        &self.beacon_entries
    }
    /// Getter for winning PoSt proof
    pub fn winning_post_proof(&self) -> &[PoStProof] {
        &self.winning_post_proof
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
    pub fn ticket(&self) -> &Option<Ticket> {
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
    /// Getter for BlockHeader parent_base_fee
    pub fn parent_base_fee(&self) -> &BigInt {
        &self.parent_base_fee
    }
    /// Getter for BlockHeader fork_signal
    pub fn fork_signal(&self) -> u64 {
        self.fork_signal
    }
    /// Getter for BlockHeader epost_verify
    pub fn election_proof(&self) -> &Option<ElectionProof> {
        &self.election_proof
    }
    /// Getter for BlockHeader signature
    pub fn signature(&self) -> &Option<Signature> {
        &self.signature
    }
    /// Key used for sorting headers and blocks.
    pub fn to_sort_key(&self) -> Option<([u8; 32], Vec<u8>)> {
        let ticket_hash = blake2b_256(self.ticket().as_ref()?.vrfproof.as_bytes());
        Some((ticket_hash, self.cid().to_bytes()))
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
            .verify(&self.cid().to_bytes(), &addr)
            .map_err(|e| Error::InvalidSignature(format!("Block signature invalid: {}", e)))?;

        Ok(())
    }
    /// Returns true if (h(vrfout) * totalPower) < (e * sectorSize * 2^256)
    pub fn is_ticket_winner(ticket: &Ticket, mpow: BigInt, net_pow: BigInt) -> bool {
        /*
        Need to check that
        (h(vrfout) + 1) / (max(h) + 1) <= e * myPower / totalPower
        max(h) == 2^256-1
        which in terms of integer math means:
        (h(vrfout) + 1) * totalPower <= e * myPower * 2^256
        in 2^256 space, it is equivalent to:
        h(vrfout) * totalPower < e * myPower * 2^256
        */

        let h = sha2::Sha256::digest(ticket.vrfproof.as_bytes());
        let mut lhs = BigInt::from_signed_bytes_be(&h);
        lhs *= net_pow;

        // rhs = sectorSize * 2^256
        // rhs = sectorSize << 256
        let mut rhs = mpow << SHA_256_BITS;
        rhs *= BigInt::from(BLOCKS_PER_EPOCH);

        // h(vrfout) * totalPower < e * sectorSize * 2^256
        lhs < rhs
    }

    /// Validates if the current header's Beacon entries are valid to ensure randomness was generated correctly
    pub async fn validate_block_drand<B: Beacon>(
        &self,
        beacon: &B,
        prev_entry: &BeaconEntry,
    ) -> Result<(), Error> {
        // TODO validation may need to use the beacon schedule from `ChainSyncer`. Seems outdated
        let max_round = beacon.max_beacon_round_for_epoch(self.epoch);
        if max_round == prev_entry.round() {
            if !self.beacon_entries.is_empty() {
                return Err(Error::Validation(format!(
                    "expected not to have any beacon entries in this block, got: {:?}",
                    self.beacon_entries.len()
                )));
            }
            return Ok(());
        }

        let last = self.beacon_entries.last().unwrap();
        if last.round() != max_round {
            return Err(Error::Validation(format!(
                "expected final beacon entry in block to be at round {}, got: {}",
                max_round,
                last.round()
            )));
        }

        let mut prev = prev_entry;
        for curr in &self.beacon_entries {
            if !beacon
                .verify_entry(&curr, &prev)
                .await
                .map_err(|e| Error::Validation(e.to_string()))?
            {
                return Err(Error::Validation(format!(
                    "beacon entry was invalid: curr:{:?}, prev: {:?}",
                    curr, prev
                )));
            }
            prev = &curr;
        }
        Ok(())
    }
    /// Serializes the header to bytes for signing purposes i.e. without the signature field
    pub fn to_signing_bytes(&self) -> Result<Vec<u8>, String> {
        let mut blk = self.clone();
        blk.signature = None;
        blk.marshal_cbor().map_err(|e| e.to_string())
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
        // This test vector is the genesis header for interopnet config
        let bz = hex::decode("904300e80781586082cb7477a801f55c1f2ea5e5d1167661feea60a39f697e1099af132682b81cc5047beacf5b6e80d5f52b9fd90323fb8510a5396416dd076c13c85619e176558582744053a3faef6764829aa02132a1571a76aabdc498a638ea0054d3bb57f41d82015860812d2396cc4592cdf7f829374b01ffd03c5469a4b0a9acc5ccc642797aa0a5498b97b28d90820fedc6f79ff0a6005f5c15dbaca3b8a45720af7ed53000555667207a0ccb50073cd24510995abd4c4e45c1e9e114905018b2da9454190499941e818201582012dd0a6a7d0e222a97926da03adb5a7768d31cc7c5c2bd6828e14a7d25fa3a608182004b76616c69642070726f6f6681d82a5827000171a0e4022030f89a8b0373ad69079dbcbc5addfe9b34dce932189786e50d3eb432ede3ba9c43000f0001d82a5827000171a0e4022052238c7d15c100c1b9ebf849541810c9e3c2d86e826512c6c416d2318fcd496dd82a5827000171a0e40220e5658b3d18cd06e1db9015b4b0ec55c123a24d5be1ea24d83938c5b8397b4f2fd82a5827000171a0e4022018d351341c302a21786b585708c9873565a0d07c42521d4aaf52da3ff6f2e461586102c000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001a5f2c5439586102b5cd48724dce0fec8799d77fd6c5113276e7f470c8391faa0b5a6033a3eaf357d635705c36abe10309d73592727289680515afd9d424793ba4796b052682d21b03c5c8a37d94827fecc59cdc5750e198fdf20dee012f4d627c6665132298ab95004500053724e0").unwrap();
        let header = BlockHeader::unmarshal_cbor(&bz).unwrap();
        assert_eq!(hex::encode(header.marshal_cbor().unwrap()), hex::encode(bz));
    }
}
