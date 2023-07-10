// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::fmt;

use crate::beacon::{Beacon, BeaconEntry, BeaconSchedule};
use crate::shim::clock::ChainEpoch;
use crate::shim::{
    address::Address,
    bigint::{BigIntDe, BigIntSer},
    crypto::Signature,
    econ::TokenAmount,
    sector::PoStProof,
    version::NetworkVersion,
};
use crate::utils::{cid::CidCborExt, encoding::blake2b_256};
use cid::Cid;
use derive_builder::Builder;
use num::BigInt;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::{ElectionProof, Error, Ticket, TipsetKeys};

pub mod json;

/// Header of a block
///
/// Usage:
/// ```
/// # use forest_filecoin::doctest_private::{BlockHeader, TipsetKeys, Ticket, Signature, Address};
/// use cid::Cid;
/// use cid::multihash::Code::Identity;
/// use num::BigInt;
/// use fvm_ipld_encoding::DAG_CBOR;
/// use cid::multihash::MultihashDigest;
///
/// BlockHeader::builder()
///     .messages(Cid::new_v1(DAG_CBOR, Identity.digest(&[]))) // required
///     .message_receipts(Cid::new_v1(DAG_CBOR, Identity.digest(&[]))) // required
///     .state_root(Cid::new_v1(DAG_CBOR, Identity.digest(&[]))) // required
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
///     .build()
///     .unwrap();
/// ```
#[derive(Clone, Debug, Builder)]
#[builder(name = "BlockHeaderBuilder")]
pub struct BlockHeader {
    // CHAIN LINKING
    /// Parents is the set of parents this block was based on. Typically one,
    /// but can be several in the case where there were multiple winning ticket-
    /// holders for an epoch
    #[builder(default)]
    parents: TipsetKeys,

    /// `weight` is the aggregate chain weight of the parent set
    #[builder(default)]
    weight: BigInt,

    /// `epoch` is the period in which a new block is generated.
    /// There may be multiple rounds in an epoch.
    #[builder(default)]
    epoch: ChainEpoch,

    /// `beacon_entries` contain the verifiable oracle randomness used to elect
    /// this block's author leader
    #[builder(default)]
    beacon_entries: Vec<BeaconEntry>,

    /// `PoStProofs` are the winning post proofs
    #[builder(default)]
    winning_post_proof: Vec<PoStProof>,

    // MINER INFO
    /// `miner_address` is the address of the miner actor that mined this block
    miner_address: Address,

    // STATE
    /// `messages` contains the `cid` to the Merkle links for `bls_messages` and
    /// `secp_messages`
    #[builder(default)]
    messages: Cid,

    /// `message_receipts` is the `cid` of the root of an array of
    /// `MessageReceipts`
    #[builder(default)]
    message_receipts: Cid,

    /// `state_root` is a `cid` pointer to the parent state root after
    /// calculating parent tipset.
    #[builder(default)]
    state_root: Cid,

    #[builder(default)]
    fork_signal: u64,

    #[builder(default)]
    pub signature: Option<Signature>,

    #[builder(default)]
    election_proof: Option<ElectionProof>,

    // CONSENSUS
    /// timestamp, in seconds since the Unix epoch, at which this block was
    /// created
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
    /// stores the `cid` for the block after the first call to `cid()`
    #[builder(default, setter(skip))]
    cached_cid: OnceCell<Cid>,

    /// Cached signature validation
    #[builder(setter(skip), default)]
    is_validated: OnceCell<bool>,
}

impl PartialEq for BlockHeader {
    fn eq(&self, other: &Self) -> bool {
        self.cid().eq(other.cid())
    }
}

#[cfg(test)]
impl quickcheck::Arbitrary for BlockHeader {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        // XXX: More fields can be randomly generated.
        let block_header = BlockHeader::builder()
            .miner_address(Address::new_id(0))
            .epoch(ChainEpoch::arbitrary(g))
            .build()
            .unwrap();
        block_header
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
            &self.parent_base_fee,
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
            parent_base_fee,
        ) = Deserialize::deserialize(deserializer)?;

        let header = BlockHeader {
            parents,
            weight,
            epoch,
            beacon_entries,
            winning_post_proof,
            miner_address,
            messages,
            message_receipts,
            state_root,
            fork_signal,
            signature,
            election_proof,
            timestamp,
            ticket,
            bls_aggregate,
            parent_base_fee,
            cached_cid: Default::default(),
            is_validated: Default::default(),
        };

        Ok(header)
    }
}

impl BlockHeader {
    /// Generates a `BlockHeader` builder as a constructor
    pub fn builder() -> BlockHeaderBuilder {
        BlockHeaderBuilder::default()
    }
    /// Get `BlockHeader` parents
    pub fn parents(&self) -> &TipsetKeys {
        &self.parents
    }
    /// Get `BlockHeader` weight
    pub fn weight(&self) -> &BigInt {
        &self.weight
    }
    /// Get `BlockHeader` epoch
    pub fn epoch(&self) -> ChainEpoch {
        self.epoch
    }
    /// Get `Drand` `BeaconEntry`
    pub fn beacon_entries(&self) -> &[BeaconEntry] {
        &self.beacon_entries
    }
    /// Get winning `PoSt` proof
    pub fn winning_post_proof(&self) -> &[PoStProof] {
        &self.winning_post_proof
    }
    /// Get `BlockHeader.miner_address`
    pub fn miner_address(&self) -> &Address {
        &self.miner_address
    }
    /// Get `BlockHeader.messages`
    pub fn messages(&self) -> &Cid {
        &self.messages
    }
    /// Get `BlockHeader.message_receipts`
    pub fn message_receipts(&self) -> &Cid {
        &self.message_receipts
    }
    /// Get `BlockHeader.state_root`
    pub fn state_root(&self) -> &Cid {
        &self.state_root
    }
    /// Get `BlockHeader.timestamp`
    pub fn timestamp(&self) -> u64 {
        self.timestamp
    }
    /// Get `BlockHeader.ticket`
    pub fn ticket(&self) -> &Option<Ticket> {
        &self.ticket
    }
    /// Get `BlockHeader.bls_aggregate`
    pub fn bls_aggregate(&self) -> &Option<Signature> {
        &self.bls_aggregate
    }
    /// Get `BlockHeader.cid`
    pub fn cid(&self) -> &Cid {
        self.cached_cid.get_or_init(|| {
            Cid::from_cbor_blake2b256(self)
                .expect("internal error - block serialization may not fail")
        })
    }
    /// Get `BlockHeader.parent_base_fee`
    pub fn parent_base_fee(&self) -> &TokenAmount {
        &self.parent_base_fee
    }
    /// Get `BlockHeader.fork_signal`
    pub fn fork_signal(&self) -> u64 {
        self.fork_signal
    }
    /// Get `BlockHeader.election_proof`
    pub fn election_proof(&self) -> &Option<ElectionProof> {
        &self.election_proof
    }
    /// Get `BlockHeader.signature`
    pub fn signature(&self) -> &Option<Signature> {
        &self.signature
    }
    /// Key used for sorting headers and blocks.
    pub fn to_sort_key(&self) -> Option<([u8; 32], Vec<u8>)> {
        let ticket_hash = blake2b_256(self.ticket().as_ref()?.vrfproof.as_bytes());
        Some((ticket_hash, self.cid().to_bytes()))
    }
    /// Check to ensure block signature is valid
    pub fn check_block_signature(&self, addr: &Address) -> Result<(), Error> {
        // If the block has already been validated, short circuit
        if let Some(true) = self.is_validated.get() {
            return Ok(());
        }

        let signature = self
            .signature()
            .as_ref()
            .ok_or_else(|| Error::InvalidSignature("Signature is nil in header".to_owned()))?;

        signature
            .verify(&self.to_signing_bytes(), addr)
            .map_err(|e| Error::InvalidSignature(format!("Block signature invalid: {e}")))?;

        // Set validated cache to true
        let _ = self.is_validated.set(true);

        Ok(())
    }

    /// Validates if the current header's Beacon entries are valid to ensure
    /// randomness was generated correctly
    pub fn validate_block_drand<B: Beacon>(
        &self,
        network_version: NetworkVersion,
        b_schedule: &BeaconSchedule<B>,
        parent_epoch: ChainEpoch,
        prev_entry: &BeaconEntry,
    ) -> Result<(), Error> {
        let (cb_epoch, curr_beacon) = b_schedule
            .beacon_for_epoch(self.epoch)
            .map_err(|e| Error::Validation(e.to_string()))?;
        let (pb_epoch, _) = b_schedule
            .beacon_for_epoch(parent_epoch)
            .map_err(|e| Error::Validation(e.to_string()))?;

        if cb_epoch != pb_epoch {
            // Fork logic
            if self.beacon_entries.len() != 2 {
                return Err(Error::Validation(format!(
                    "Expected two beacon entries at beacon fork, got {}",
                    self.beacon_entries.len()
                )));
            }

            curr_beacon
                .verify_entry(&self.beacon_entries[1], &self.beacon_entries[0])
                .map_err(|e| Error::Validation(e.to_string()))?;

            return Ok(());
        }

        let max_round = curr_beacon.max_beacon_round_for_epoch(network_version, self.epoch);
        if max_round == prev_entry.round() {
            if !self.beacon_entries.is_empty() {
                return Err(Error::Validation(format!(
                    "expected not to have any beacon entries in this block, got: {:?}",
                    self.beacon_entries.len()
                )));
            }
            return Ok(());
        }

        let last = match self.beacon_entries.last() {
            Some(last) => last,
            None => {
                return Err(Error::Validation(
                    "Block must include at least 1 beacon entry".to_string(),
                ));
            }
        };
        if last.round() != max_round {
            return Err(Error::Validation(format!(
                "expected final beacon entry in block to be at round {}, got: {}",
                max_round,
                last.round()
            )));
        }

        let mut prev = prev_entry;
        for curr in &self.beacon_entries {
            if !curr_beacon
                .verify_entry(curr, prev)
                .map_err(|e| Error::Validation(e.to_string()))?
            {
                return Err(Error::Validation(format!(
                    "beacon entry was invalid: curr:{curr:?}, prev: {prev:?}"
                )));
            }
            prev = curr;
        }
        Ok(())
    }

    /// Serializes the header to bytes for signing purposes i.e. without the
    /// signature field
    pub fn to_signing_bytes(&self) -> Vec<u8> {
        let mut blk = self.clone();
        blk.signature = None;

        // This isn't required now, but future proofs for if the encoding ever uses a
        // cache.
        blk.cached_cid = Default::default();

        // * Intentionally not using cache here, to avoid using cached bytes with
        //   signature encoded.
        fvm_ipld_encoding::to_vec(&blk).expect("block serialization cannot fail")
    }
}

/// human-readable string representation of a block CID
impl fmt::Display for BlockHeader {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "BlockHeader: {:?}", self.cid())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::beacon::{mock_beacon::MockBeacon, BeaconEntry, BeaconPoint, BeaconSchedule};
    use crate::shim::{address::Address, version::NetworkVersion};
    use fvm_ipld_encoding::{from_slice, to_vec};

    use crate::blocks::{errors::Error, BlockHeader};

    #[test]
    fn symmetric_header_encoding() {
        // This test vector is pulled from space race, and contains a valid signature
        let bz = hex::decode("904300e8078158608798de4e49e02ee129920224ea767650aa6e693857431cc95b5a092a57d80ef4d841ebedbf09f7680a5e286cd297f40100b496648e1fa0fd55f899a45d51404a339564e7d4809741ba41d9fcc8ac0261bf521cd5f718389e81354eff2aa52b338201586084d8929eeedc654d6bec8bb750fcc8a1ebf2775d8167d3418825d9e989905a8b7656d906d23dc83e0dad6e7f7a193df70a82d37da0565ce69b776d995eefd50354c85ec896a2173a5efed53a27275e001ad72a3317b2190b98cceb0f01c46b7b81821a00013cbe5860ae1102b76dea635b2f07b7d06e1671d695c4011a73dc33cace159509eac7edc305fa74495505f0cd0046ee0d3b17fabc0fc0560d44d296c6d91bcc94df76266a8e9d5312c617ca72a2e186cadee560477f6d120f6614e21fb07c2390a166a25981820358c0b965705cec77b46200af8fb2e47c0eca175564075061132949f00473dcbe74529c623eb510081e8b8bd34418d21c646485d893f040dcfb7a7e7af9ae4ed7bd06772c24fb0cc5b8915300ab5904fbd90269d523018fbf074620fd3060d55dd6c6057b4195950ac4155a735e8fec79767f659c30ea6ccf0813a4ab2b4e60f36c04c71fb6c58efc123f60c6ea8797ab3706a80a4ccc1c249989934a391803789ab7d04f514ee0401d0f87a1f5262399c451dcf5f7ec3bb307fc6f1a41f5ff3a5ddb81d82a5827000171a0e402209a0640d0620af5d1c458effce4cbb8969779c9072b164d3fe6f5179d6378d8cd4300310001d82a5827000171a0e402208fbc07f7587e2efebab9ff1ab27c928881abf9d1b7e5ad5206781415615867aed82a5827000171a0e40220e5658b3d18cd06e1db9015b4b0ec55c123a24d5be1ea24d83938c5b8397b4f2fd82a5827000171a0e402209967f10c4c0e336b3517d3a972f701dadea5b41ce33defb126b88e650cf884545861028ec8b64e2d93272f97edcab1f56bcad4a2b145ea88c232bfae228e4adbbd807e6a41740cc8cb569197dae6b2cbf8c1a4035e81fd7805ccbe88a5ec476bcfa438db4bd677de06b45e94310533513e9d17c635940ba8fa2650cdb34d445724c5971a5f44387e5861028a45c70a39fe8e526cbb6ba2a850e9063460873d6329f26cc2fc91972256c40249dba289830cc99619109c18e695d78012f760e7fda1b68bc3f1fe20ff8a017044753da38ca6384de652f3ee13aae5b64e6f88f85fd50d5c862fed3c1f594ace004500053724e0").unwrap();
        let header = from_slice::<BlockHeader>(&bz).unwrap();
        assert_eq!(to_vec(&header).unwrap(), bz);

        // Verify the signature of this block header using the resolved address used to
        // sign. This is a valid signature, but if the block header vector
        // changes, the address should need to as well.
        header
            .check_block_signature(
                &"f3vfs6f7tagrcpnwv65wq3leznbajqyg77bmijrpvoyjv3zjyi3urq25vigfbs3ob6ug5xdihajumtgsxnz2pa"
                .parse()
                .unwrap())
            .unwrap();
    }

    #[test]
    fn beacon_entry_exists() {
        // Setup
        let block_header = BlockHeader::builder()
            .miner_address(Address::new_id(0))
            .beacon_entries(Vec::new())
            .build()
            .unwrap();
        let beacon_schedule = Arc::new(BeaconSchedule(vec![BeaconPoint {
            height: 0,
            beacon: Arc::new(MockBeacon::default()),
        }]));
        let chain_epoch = 0;
        let beacon_entry = BeaconEntry::new(1, vec![]);
        // Validate_block_drand
        if let Err(e) = block_header.validate_block_drand(
            NetworkVersion::V16,
            &beacon_schedule,
            chain_epoch,
            &beacon_entry,
        ) {
            // Assert error is for not including a beacon entry in the block
            match e {
                Error::Validation(why) => {
                    assert_eq!(why, "Block must include at least 1 beacon entry");
                }
                _ => {
                    panic!("validate block drand must detect a beacon entry in the block header");
                }
            }
        }
    }
}
