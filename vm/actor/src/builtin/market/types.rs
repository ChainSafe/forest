// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::deal::{ClientDealProposal, DealProposal, DealState};
use crate::DealWeight;
use address::Address;
use cid::Cid;
use clock::ChainEpoch;
use encoding::tuple::*;
use fil_types::RegisteredSealProof;
use ipld_amt::Amt;
use num_bigint::bigint_ser;
use vm::{DealID, TokenAmount};

pub const PROPOSALS_AMT_BITWIDTH: usize = 5;
pub const STATES_AMT_BITWIDTH: usize = 6;

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct WithdrawBalanceParams {
    pub provider_or_client: Address,
    #[serde(with = "bigint_ser")]
    pub amount: TokenAmount,
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

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct PublishStorageDealsReturn {
    pub ids: Vec<DealID>,
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
    pub sector_expiry: ChainEpoch,
    pub deal_ids: Vec<DealID>,
}

#[derive(Serialize_tuple)]
pub struct VerifyDealsForActivationParamsRef<'a> {
    pub sectors: &'a [SectorDeals],
}

#[derive(Serialize_tuple, Deserialize_tuple, Default)]
pub struct VerifyDealsForActivationReturn {
    pub sectors: Vec<SectorWeights>,
}

#[derive(Serialize_tuple, Deserialize_tuple, Default)]
pub struct SectorWeights {
    pub deal_space: u64,
    #[serde(with = "bigint_ser")]
    pub deal_weight: DealWeight,
    #[serde(with = "bigint_ser")]
    pub verified_deal_weight: DealWeight,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct ActivateDealsParams {
    pub deal_ids: Vec<DealID>,
    pub sector_expiry: ChainEpoch,
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
pub type DealArray<'bs, BS> = Amt<'bs, DealProposal, BS>;

/// A specialization of a array to deals.
pub type DealMetaArray<'bs, BS> = Amt<'bs, DealState, BS>;

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct SectorDataSpec {
    pub deal_ids: Vec<DealID>,
    pub sector_type: RegisteredSealProof,
}
