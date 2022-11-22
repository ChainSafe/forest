// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;
use fil_actors_runtime_v9::Array;
use fvm_ipld_bitfield::BitField;
use fvm_ipld_encoding::tuple::*;
use fvm_ipld_encoding::Cbor;
use fvm_shared::address::Address;
use fvm_shared::bigint::{bigint_ser, BigInt};
use fvm_shared::clock::ChainEpoch;
use fvm_shared::deal::DealID;
use fvm_shared::econ::TokenAmount;
use fvm_shared::piece::PaddedPieceSize;
use fvm_shared::ActorID;

use fvm_shared::sector::RegisteredSealProof;

use super::deal::{ClientDealProposal, DealProposal, DealState};

pub const PROPOSALS_AMT_BITWIDTH: u32 = 5;
pub const STATES_AMT_BITWIDTH: u32 = 6;

pub type AllocationID = u64;

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct WithdrawBalanceParams {
    pub provider_or_client: Address,
    pub amount: TokenAmount,
}

impl Cbor for WithdrawBalanceParams {}

#[derive(Serialize_tuple, Deserialize_tuple)]
#[serde(transparent)]
pub struct WithdrawBalanceReturn {
    pub amount_withdrawn: TokenAmount,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct OnMinerSectorsTerminateParams {
    pub epoch: ChainEpoch,
    pub deal_ids: Vec<DealID>,
}

#[derive(Serialize_tuple)]

pub struct OnMinerSectorsTerminateParamsRef<'a> {
    pub epoch: ChainEpoch,
    pub deal_ids: &'a [DealID],
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct PublishStorageDealsParams {
    pub deals: Vec<ClientDealProposal>,
}

impl Cbor for PublishStorageDealsParams {}

#[derive(Serialize_tuple, Deserialize_tuple, Debug)]
pub struct PublishStorageDealsReturn {
    pub ids: Vec<DealID>,
    pub valid_deals: BitField,
}

// Changed since V2:
// - Array of Sectors rather than just one
// - Removed SectorStart
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct VerifyDealsForActivationParams {
    pub sectors: Vec<SectorDeals>,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct SectorDeals {
    pub sector_type: RegisteredSealProof,
    pub sector_expiry: ChainEpoch,
    pub deal_ids: Vec<DealID>,
}

#[derive(Serialize_tuple)]
pub struct VerifyDealsForActivationParamsRef<'a> {
    pub sectors: &'a [SectorDeals],
}

#[derive(Serialize_tuple, Deserialize_tuple, Default)]
pub struct VerifyDealsForActivationReturn {
    pub sectors: Vec<SectorDealData>,
}

#[derive(Serialize_tuple, Deserialize_tuple, Default, Clone)]
pub struct SectorDealData {
    /// Option::None signifies commitment to empty sector, meaning no deals.
    pub commd: Option<Cid>,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct ActivateDealsParams {
    pub deal_ids: Vec<DealID>,
    pub sector_expiry: ChainEpoch,
}

#[derive(Serialize_tuple, Deserialize_tuple, Clone)]
pub struct VerifiedDealInfo {
    pub client: ActorID,
    pub allocation_id: AllocationID,
    pub data: Cid,
    pub size: PaddedPieceSize,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct ActivateDealsResult {
    #[serde(with = "bigint_ser")]
    pub nonverified_deal_space: BigInt,
    pub verified_infos: Vec<VerifiedDealInfo>,
}
#[derive(Serialize_tuple, Deserialize_tuple, Debug, Clone, Default)]
pub struct DealSpaces {
    #[serde(with = "bigint_ser")]
    pub deal_space: BigInt,
    #[serde(with = "bigint_ser")]
    pub verified_deal_space: BigInt,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct ComputeDataCommitmentParams {
    pub inputs: Vec<SectorDataSpec>,
}

#[derive(Serialize_tuple)]
pub struct ComputeDataCommitmentParamsRef<'a> {
    pub inputs: &'a [SectorDataSpec],
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct ComputeDataCommitmentReturn {
    pub commds: Vec<Cid>,
}

/// A specialization of a array to deals.
pub type DealArray<'bs, BS> = Array<'bs, DealProposal, BS>;

/// A specialization of a array to deals.
pub type DealMetaArray<'bs, BS> = Array<'bs, DealState, BS>;

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct SectorDataSpec {
    pub deal_ids: Vec<DealID>,
    pub sector_type: RegisteredSealProof,
}
