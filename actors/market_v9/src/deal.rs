// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::types::AllocationID;
use cid::{Cid, Version};
use fvm_ipld_encoding::tuple::*;
use fvm_ipld_encoding::{BytesSer, Cbor};
use fvm_shared::address::Address;
use fvm_shared::clock::ChainEpoch;
use fvm_shared::commcid::{FIL_COMMITMENT_UNSEALED, SHA2_256_TRUNC254_PADDED};
use fvm_shared::crypto::signature::Signature;
use fvm_shared::econ::TokenAmount;
use fvm_shared::piece::PaddedPieceSize;
use libipld_core::ipld::Ipld;
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use std::convert::{TryFrom, TryInto};

/// Cid prefix for piece Cids
pub fn is_piece_cid(c: &Cid) -> bool {
    // TODO: Move FIL_COMMITMENT etc, into a better place
    c.version() == Version::V1
        && c.codec() == FIL_COMMITMENT_UNSEALED
        && c.hash().code() == SHA2_256_TRUNC254_PADDED
        && c.hash().size() == 32
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Label {
    String(String),
    Bytes(Vec<u8>),
}

/// Serialize the Label like an untagged enum.
impl Serialize for Label {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Label::String(v) => v.serialize(serializer),
            Label::Bytes(v) => BytesSer(v).serialize(serializer),
        }
    }
}

impl TryFrom<Ipld> for Label {
    type Error = String;

    fn try_from(ipld: Ipld) -> Result<Self, Self::Error> {
        match ipld {
            Ipld::String(s) => Ok(Label::String(s)),
            Ipld::Bytes(b) => Ok(Label::Bytes(b)),
            other => Err(format!(
                "Expected `Ipld::String` or `Ipld::Bytes`, got {:#?}",
                other
            )),
        }
    }
}

/// Deserialize the Label like an untagged enum.
impl<'de> Deserialize<'de> for Label {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ipld::deserialize(deserializer).and_then(|ipld| ipld.try_into().map_err(de::Error::custom))
    }
}

impl Label {
    pub fn len(&self) -> usize {
        match self {
            Label::String(s) => s.len(),
            Label::Bytes(b) => b.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            Label::String(s) => s.is_empty(),
            Label::Bytes(b) => b.is_empty(),
        }
    }
}

/// Note: Deal Collateral is only released and returned to clients and miners
/// when the storage deal stops counting towards power. In the current iteration,
/// it will be released when the sector containing the storage deals expires,
/// even though some storage deals can expire earlier than the sector does.
/// Collaterals are denominated in PerEpoch to incur a cost for self dealing or
/// minimal deals that last for a long time.
/// Note: ClientCollateralPerEpoch may not be needed and removed pending future confirmation.
/// There will be a Minimum value for both client and provider deal collateral.
#[derive(Clone, Debug, PartialEq, Eq, Serialize_tuple, Deserialize_tuple)]
pub struct DealProposal {
    pub piece_cid: Cid,
    pub piece_size: PaddedPieceSize,
    pub verified_deal: bool,
    pub client: Address,
    pub provider: Address,

    /// Arbitrary client chosen label to apply to the deal
    // ! This is the field that requires unsafe unchecked utf8 deserialization
    pub label: Label,

    // Nominal start epoch. Deal payment is linear between StartEpoch and EndEpoch,
    // with total amount StoragePricePerEpoch * (EndEpoch - StartEpoch).
    // Storage deal must appear in a sealed (proven) sector no later than StartEpoch,
    // otherwise it is invalid.
    pub start_epoch: ChainEpoch,
    pub end_epoch: ChainEpoch,
    pub storage_price_per_epoch: TokenAmount,

    pub provider_collateral: TokenAmount,
    pub client_collateral: TokenAmount,
}

impl Cbor for DealProposal {}

impl DealProposal {
    pub fn duration(&self) -> ChainEpoch {
        self.end_epoch - self.start_epoch
    }
    pub fn total_storage_fee(&self) -> TokenAmount {
        self.storage_price_per_epoch.clone() * self.duration() as u64
    }
    pub fn client_balance_requirement(&self) -> TokenAmount {
        &self.client_collateral + self.total_storage_fee()
    }
    pub fn provider_balance_requirement(&self) -> &TokenAmount {
        &self.provider_collateral
    }
}

/// ClientDealProposal is a DealProposal signed by a client
#[derive(Clone, Debug, PartialEq, Eq, Serialize_tuple, Deserialize_tuple)]
pub struct ClientDealProposal {
    pub proposal: DealProposal,
    pub client_signature: Signature,
}

impl Cbor for ClientDealProposal {}

#[derive(Clone, Debug, PartialEq, Eq, Copy, Serialize_tuple, Deserialize_tuple)]
pub struct DealState {
    // -1 if not yet included in proven sector
    pub sector_start_epoch: ChainEpoch,
    // -1 if deal state never updated
    pub last_updated_epoch: ChainEpoch,
    // -1 if deal never slashed
    pub slash_epoch: ChainEpoch,
    // ID of the verified registry allocation/claim for this deal's data (0 if none).
    pub verified_claim: AllocationID,
}
