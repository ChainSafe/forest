// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::ops::Deref;
use std::sync::atomic::{AtomicBool, Ordering};

use super::{ElectionProof, Error, Ticket, TipsetKeys};
use crate::beacon::{BeaconEntry, BeaconSchedule};
use crate::shim::clock::ChainEpoch;
use crate::shim::{
    address::Address, crypto::Signature, econ::TokenAmount, sector::PoStProof,
    version::NetworkVersion,
};
use crate::utils::{cid::CidCborExt as _, encoding::blake2b_256};
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::CborStore as _;
use num::BigInt;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use serde_tuple::{Deserialize_tuple, Serialize_tuple};

#[cfg(not(doc))]
mod lotus_json;
#[cfg(doc)]
pub mod lotus_json;
#[cfg(test)]
mod tests;

#[derive(Deserialize_tuple, Serialize_tuple, Default, Clone, Hash, Eq, PartialEq, Debug)]
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
    pub parents: TipsetKeys,
    /// The aggregate chain weight of the parent set
    #[serde(with = "fvm_shared4::bigint::bigint_ser")]
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
        Cid::from_cbor_blake2b256(self).unwrap()
    }
    /// Key used for sorting headers and blocks.
    pub fn to_sort_key(&self) -> Option<([u8; 32], Vec<u8>)> {
        let ticket_hash = blake2b_256(self.ticket.as_ref()?.vrfproof.as_bytes());
        Some((ticket_hash, self.cid().to_bytes()))
    }
    /// Check to ensure block signature is valid
    // TODO(aatifsyed): rename to `validate_signature`
    pub fn check_block_signature(&self, addr: &Address) -> Result<(), Error> {
        let signature = self
            .signature
            .as_ref()
            .ok_or_else(|| Error::InvalidSignature("Signature is nil in header".to_owned()))?;

        signature
            .verify(&self.to_signing_bytes(), addr)
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
    // TODO(aatifsyed): rename to `signing_bytes`
    pub fn to_signing_bytes(&self) -> Vec<u8> {
        let mut blk = self.clone();
        blk.signature = None;
        fvm_ipld_encoding::to_vec(&blk).expect("block serialization cannot fail")
    }
}

#[derive(Debug, Default)]
pub struct BlockHeader {
    uncached: RawBlockHeader,
    cid: OnceCell<Cid>,
    // TODO(aatifsyed): I'm pretty this shouldn't be cached - it used to be called `is_validated`
    signature_has_ever_been_checked: AtomicBool,
}

impl PartialEq for BlockHeader {
    fn eq(&self, other: &Self) -> bool {
        // TODO(aatifsyed): ouch
        self.uncached == other.uncached
    }
}

impl Clone for BlockHeader {
    fn clone(&self) -> Self {
        Self {
            uncached: self.uncached.clone(),
            cid: self.cid.clone(),
            signature_has_ever_been_checked: AtomicBool::new(
                self.signature_has_ever_been_checked.load(Ordering::Acquire),
            ),
        }
    }
}

impl Deref for BlockHeader {
    type Target = RawBlockHeader;

    fn deref(&self) -> &Self::Target {
        &self.uncached
    }
}

impl BlockHeader {
    pub fn new(uncached: RawBlockHeader) -> Self {
        Self {
            uncached,
            cid: OnceCell::new(),
            signature_has_ever_been_checked: AtomicBool::new(false),
        }
    }
    /// Returns [`None`] if the blockstore doesn't contain the CID.
    pub fn load(store: impl Blockstore, cid: Cid) -> anyhow::Result<Option<Self>> {
        if let Some(uncached) = store.get_cbor::<RawBlockHeader>(&cid)? {
            Ok(Some(Self {
                uncached,
                cid: OnceCell::with_value(cid),
                signature_has_ever_been_checked: AtomicBool::new(false),
            }))
        } else {
            Ok(None)
        }
    }
    pub fn cid(&self) -> &Cid {
        self.cid.get_or_init(|| self.uncached.cid())
    }

    pub fn check_block_signature(&self, addr: &Address) -> Result<(), Error> {
        match self.signature_has_ever_been_checked.load(Ordering::Acquire) {
            true => Ok(()),
            false => match self.uncached.check_block_signature(addr) {
                Ok(()) => {
                    self.signature_has_ever_been_checked
                        .store(true, Ordering::Release);
                    Ok(())
                }
                Err(e) => Err(e),
            },
        }
    }
}

impl Serialize for BlockHeader {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.uncached.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for BlockHeader {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        RawBlockHeader::deserialize(deserializer).map(Self::new)
    }
}
