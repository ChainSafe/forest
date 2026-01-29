// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::{
    OnceLock,
    atomic::{AtomicBool, Ordering},
};

use super::{ElectionProof, Error, Ticket, TipsetKey};
use crate::{
    beacon::{BeaconEntry, BeaconSchedule},
    shim::{
        address::Address, clock::ChainEpoch, crypto::Signature, econ::TokenAmount,
        sector::PoStProof, version::NetworkVersion,
    },
    utils::{encoding::blake2b_256, get_size::big_int_heap_size_helper, multihash::MultihashCode},
};
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::CborStore as _;
use fvm_ipld_encoding::tuple::*;
use get_size2::GetSize;
use multihash_derive::MultihashDigest as _;
use num::BigInt;
use serde::{Deserialize, Serialize};

#[cfg(test)]
mod test;
#[cfg(test)]
pub use test::*;

#[derive(Deserialize_tuple, Serialize_tuple, Clone, Hash, Eq, PartialEq, Debug)]
pub struct RawBlockHeader {
    /// The address of the miner actor that mined this block
    pub miner_address: Address,
    pub ticket: Option<Ticket>,
    pub election_proof: Option<ElectionProof>,
    /// The verifiable oracle randomness used to elect this block's author leader
    pub beacon_entries: Vec<BeaconEntry>,
    pub winning_post_proof: Vec<PoStProof>,
    /// The set of parents this block was based on.
    /// Typically one, but can be several in the case where there were multiple
    /// winning ticket-holders for an epoch
    pub parents: TipsetKey,
    /// The aggregate chain weight of the parent set
    #[serde(with = "crate::shim::fvm_shared_latest::bigint::bigint_ser")]
    pub weight: BigInt,
    /// The period in which a new block is generated.
    /// There may be multiple rounds in an epoch.
    pub epoch: ChainEpoch,
    /// The CID of the parent state root after calculating parent tipset.
    pub state_root: Cid,
    /// The CID of the root of an array of `MessageReceipts`
    pub message_receipts: Cid,
    /// The CID of the Merkle links for `bls_messages` and `secp_messages`
    pub messages: Cid,
    /// Aggregate signature of miner in block
    pub bls_aggregate: Option<Signature>,
    /// Block creation time, in seconds since the Unix epoch
    pub timestamp: u64,
    pub signature: Option<Signature>,
    pub fork_signal: u64,
    /// The base fee of the parent block
    pub parent_base_fee: TokenAmount,
}

impl RawBlockHeader {
    pub fn cid(&self) -> Cid {
        self.car_block().expect("CBOR serialization failed").0
    }
    pub fn car_block(&self) -> anyhow::Result<(Cid, Vec<u8>)> {
        let data = fvm_ipld_encoding::to_vec(self)?;
        let cid = Cid::new_v1(
            fvm_ipld_encoding::DAG_CBOR,
            MultihashCode::Blake2b256.digest(&data),
        );
        Ok((cid, data))
    }
    pub(super) fn tipset_sort_key(&self) -> Option<([u8; 32], Vec<u8>)> {
        let ticket_hash = blake2b_256(self.ticket.as_ref()?.vrfproof.as_bytes());
        Some((ticket_hash, self.cid().to_bytes()))
    }
    /// Check to ensure block signature is valid
    pub fn verify_signature_against(&self, addr: &Address) -> Result<(), Error> {
        let signature = self
            .signature
            .as_ref()
            .ok_or_else(|| Error::InvalidSignature("Signature is nil in header".to_owned()))?;

        signature
            .verify(&self.signing_bytes(), addr)
            .map_err(|e| Error::InvalidSignature(format!("Block signature invalid: {e}")))?;

        Ok(())
    }

    /// Validates if the current header's Beacon entries are valid to ensure
    /// randomness was generated correctly
    pub fn validate_block_drand(
        &self,
        network_version: NetworkVersion,
        b_schedule: &BeaconSchedule,
        parent_epoch: ChainEpoch,
        prev_entry: &BeaconEntry,
    ) -> Result<(), Error> {
        let (cb_epoch, curr_beacon) = b_schedule
            .beacon_for_epoch(self.epoch)
            .map_err(|e| Error::Validation(e.to_string()))?;
        tracing::trace!(
            "beacon network at {}: {:?}, is_chained: {}",
            self.epoch,
            curr_beacon.network(),
            curr_beacon.network().is_chained()
        );
        // Before quicknet upgrade, we had "chained" beacons, and so required two entries at a fork
        if curr_beacon.network().is_chained() {
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

                #[allow(clippy::indexing_slicing)]
                curr_beacon
                    .verify_entries(&self.beacon_entries[1..], &self.beacon_entries[0])
                    .map_err(|e| Error::Validation(e.to_string()))?;

                return Ok(());
            }
        }

        let max_round = curr_beacon.max_beacon_round_for_epoch(network_version, self.epoch);
        // We don't expect to ever actually meet this condition
        if max_round == prev_entry.round() {
            if !self.beacon_entries.is_empty() {
                return Err(Error::Validation(format!(
                    "expected not to have any beacon entries in this block, got: {}",
                    self.beacon_entries.len()
                )));
            }
            return Ok(());
        }

        // We skip verifying the genesis entry when randomness is "chained".
        if curr_beacon.network().is_chained() && prev_entry.round() == 0 {
            // This basically means that the drand entry of the first non-genesis tipset isn't verified IF we are starting on Drand mainnet (the "chained" drand)
            // Networks that start on drand quicknet, or other unchained randomness sources, will still verify it
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

        if !curr_beacon
            .verify_entries(&self.beacon_entries, prev_entry)
            .map_err(|e| Error::Validation(e.to_string()))?
        {
            return Err(Error::Validation("beacon entry was invalid".into()));
        }

        Ok(())
    }

    /// Serializes the header to bytes for signing purposes i.e. without the
    /// signature field
    pub fn signing_bytes(&self) -> Vec<u8> {
        let mut blk = self.clone();
        blk.signature = None;
        fvm_ipld_encoding::to_vec(&blk).expect("block serialization cannot fail")
    }
}

// The derive macro does not compile for some reason
impl GetSize for RawBlockHeader {
    fn get_heap_size(&self) -> usize {
        let Self {
            miner_address,
            ticket,
            election_proof,
            beacon_entries,
            winning_post_proof,
            parents,
            weight,
            epoch: _,
            state_root: _,
            message_receipts: _,
            messages: _,
            bls_aggregate,
            timestamp: _,
            signature,
            fork_signal: _,
            parent_base_fee,
        } = self;
        miner_address.get_heap_size()
            + ticket.get_heap_size()
            + election_proof.get_heap_size()
            + beacon_entries.get_heap_size()
            + winning_post_proof.get_heap_size()
            + parents.get_heap_size()
            + big_int_heap_size_helper(weight)
            + bls_aggregate.get_heap_size()
            + signature.get_heap_size()
            + parent_base_fee.get_heap_size()
    }
}

/// A [`RawBlockHeader`] which caches calls to [`RawBlockHeader::cid`] and [`RawBlockHeader::verify_signature_against`]
#[cfg_attr(test, derive(Default))]
#[derive(Debug, GetSize, derive_more::Deref)]
pub struct CachingBlockHeader {
    #[deref]
    uncached: RawBlockHeader,
    #[get_size(ignore)]
    cid: OnceLock<Cid>,
    has_ever_been_verified_against_any_signature: AtomicBool,
}

impl PartialEq for CachingBlockHeader {
    fn eq(&self, other: &Self) -> bool {
        // Epoch check is redundant but cheap.
        self.uncached.epoch == other.uncached.epoch && self.cid() == other.cid()
    }
}

impl Eq for CachingBlockHeader {}

impl Clone for CachingBlockHeader {
    fn clone(&self) -> Self {
        Self {
            uncached: self.uncached.clone(),
            cid: self.cid.clone(),
            has_ever_been_verified_against_any_signature: AtomicBool::new(
                self.has_ever_been_verified_against_any_signature
                    .load(Ordering::Acquire),
            ),
        }
    }
}

impl From<RawBlockHeader> for CachingBlockHeader {
    fn from(value: RawBlockHeader) -> Self {
        Self::new(value)
    }
}

impl CachingBlockHeader {
    pub fn new(uncached: RawBlockHeader) -> Self {
        Self {
            uncached,
            cid: OnceLock::new(),
            has_ever_been_verified_against_any_signature: AtomicBool::new(false),
        }
    }
    pub fn into_raw(self) -> RawBlockHeader {
        self.uncached
    }
    /// Returns [`None`] if the blockstore doesn't contain the CID.
    pub fn load(store: &impl Blockstore, cid: Cid) -> anyhow::Result<Option<Self>> {
        if let Some(uncached) = store.get_cbor::<RawBlockHeader>(&cid)? {
            Ok(Some(Self {
                uncached,
                cid: cid.into(),
                has_ever_been_verified_against_any_signature: AtomicBool::new(false),
            }))
        } else {
            Ok(None)
        }
    }
    pub fn cid(&self) -> &Cid {
        self.cid.get_or_init(|| self.uncached.cid())
    }

    pub fn verify_signature_against(&self, addr: &Address) -> Result<(), Error> {
        match self
            .has_ever_been_verified_against_any_signature
            .load(Ordering::Acquire)
        {
            true => Ok(()),
            false => match self.uncached.verify_signature_against(addr) {
                Ok(()) => {
                    self.has_ever_been_verified_against_any_signature
                        .store(true, Ordering::Release);
                    Ok(())
                }
                Err(e) => Err(e),
            },
        }
    }
}

impl From<CachingBlockHeader> for RawBlockHeader {
    fn from(value: CachingBlockHeader) -> Self {
        value.into_raw()
    }
}

impl Serialize for CachingBlockHeader {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.uncached.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for CachingBlockHeader {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        RawBlockHeader::deserialize(deserializer).map(Self::new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::beacon::{BeaconEntry, BeaconPoint, BeaconSchedule, mock_beacon::MockBeacon};
    use crate::blocks::{CachingBlockHeader, Error};
    use crate::shim::clock::ChainEpoch;
    use crate::shim::{address::Address, version::NetworkVersion};
    use crate::utils::encoding::from_slice_with_fallback;
    use crate::utils::multihash::MultihashCode;
    use cid::Cid;
    use fvm_ipld_encoding::{DAG_CBOR, to_vec};

    impl quickcheck::Arbitrary for CachingBlockHeader {
        fn arbitrary(g: &mut quickcheck::Gen) -> Self {
            // TODO(forest): https://github.com/ChainSafe/forest/issues/3571
            CachingBlockHeader::new(RawBlockHeader {
                miner_address: Address::new_id(0),
                epoch: ChainEpoch::arbitrary(g),
                ..Default::default()
            })
        }
    }

    #[test]
    fn symmetric_header_encoding() {
        // This test vector is pulled from space race, and contains a valid signature
        let bz = hex::decode("904300e8078158608798de4e49e02ee129920224ea767650aa6e693857431cc95b5a092a57d80ef4d841ebedbf09f7680a5e286cd297f40100b496648e1fa0fd55f899a45d51404a339564e7d4809741ba41d9fcc8ac0261bf521cd5f718389e81354eff2aa52b338201586084d8929eeedc654d6bec8bb750fcc8a1ebf2775d8167d3418825d9e989905a8b7656d906d23dc83e0dad6e7f7a193df70a82d37da0565ce69b776d995eefd50354c85ec896a2173a5efed53a27275e001ad72a3317b2190b98cceb0f01c46b7b81821a00013cbe5860ae1102b76dea635b2f07b7d06e1671d695c4011a73dc33cace159509eac7edc305fa74495505f0cd0046ee0d3b17fabc0fc0560d44d296c6d91bcc94df76266a8e9d5312c617ca72a2e186cadee560477f6d120f6614e21fb07c2390a166a25981820358c0b965705cec77b46200af8fb2e47c0eca175564075061132949f00473dcbe74529c623eb510081e8b8bd34418d21c646485d893f040dcfb7a7e7af9ae4ed7bd06772c24fb0cc5b8915300ab5904fbd90269d523018fbf074620fd3060d55dd6c6057b4195950ac4155a735e8fec79767f659c30ea6ccf0813a4ab2b4e60f36c04c71fb6c58efc123f60c6ea8797ab3706a80a4ccc1c249989934a391803789ab7d04f514ee0401d0f87a1f5262399c451dcf5f7ec3bb307fc6f1a41f5ff3a5ddb81d82a5827000171a0e402209a0640d0620af5d1c458effce4cbb8969779c9072b164d3fe6f5179d6378d8cd4300310001d82a5827000171a0e402208fbc07f7587e2efebab9ff1ab27c928881abf9d1b7e5ad5206781415615867aed82a5827000171a0e40220e5658b3d18cd06e1db9015b4b0ec55c123a24d5be1ea24d83938c5b8397b4f2fd82a5827000171a0e402209967f10c4c0e336b3517d3a972f701dadea5b41ce33defb126b88e650cf884545861028ec8b64e2d93272f97edcab1f56bcad4a2b145ea88c232bfae228e4adbbd807e6a41740cc8cb569197dae6b2cbf8c1a4035e81fd7805ccbe88a5ec476bcfa438db4bd677de06b45e94310533513e9d17c635940ba8fa2650cdb34d445724c5971a5f44387e5861028a45c70a39fe8e526cbb6ba2a850e9063460873d6329f26cc2fc91972256c40249dba289830cc99619109c18e695d78012f760e7fda1b68bc3f1fe20ff8a017044753da38ca6384de652f3ee13aae5b64e6f88f85fd50d5c862fed3c1f594ace004500053724e0").unwrap();
        let header = from_slice_with_fallback::<CachingBlockHeader>(&bz).unwrap();
        assert_eq!(to_vec(&header).unwrap(), bz);

        // Verify the signature of this block header using the resolved address used to
        // sign. This is a valid signature, but if the block header vector
        // changes, the address should need to as well.
        header
            .verify_signature_against(
                &"f3vfs6f7tagrcpnwv65wq3leznbajqyg77bmijrpvoyjv3zjyi3urq25vigfbs3ob6ug5xdihajumtgsxnz2pa"
                .parse()
                .unwrap())
            .unwrap();
    }

    #[test]
    fn beacon_entry_exists() {
        // Setup
        let block_header = CachingBlockHeader::new(RawBlockHeader {
            miner_address: Address::new_id(0),
            ..Default::default()
        });
        let beacon_schedule = BeaconSchedule(vec![BeaconPoint {
            height: 0,
            beacon: Box::<MockBeacon>::default(),
        }]);
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

    #[test]
    fn test_genesis_parent() {
        assert_eq!(
            Cid::new_v1(
                DAG_CBOR,
                MultihashCode::Sha2_256.digest(&FILECOIN_GENESIS_BLOCK)
            ),
            *FILECOIN_GENESIS_CID
        );
    }
}
