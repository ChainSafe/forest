// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::deal::ClientDealProposal;
use crate::DealWeight;
use address::Address;
use clock::ChainEpoch;
use encoding::tuple::*;
use fil_types::RegisteredSealProof;
use num_bigint::bigint_ser;
use num_bigint::biguint_ser;
use vm::{DealID, TokenAmount};

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct WithdrawBalanceParams {
    pub provider_or_client: Address,
    #[serde(with = "bigint_ser")]
    pub amount: TokenAmount,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct OnMinerSectorsTerminateParams {
    pub deal_ids: Vec<DealID>,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct HandleExpiredDealsParams {
    pub deal_ids: Vec<DealID>,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct PublishStorageDealsParams {
    pub deals: Vec<ClientDealProposal>,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct PublishStorageDealsReturn {
    pub ids: Vec<DealID>,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct VerifyDealsOnSectorProveCommitParams {
    pub deal_ids: Vec<DealID>,
    pub sector_expiry: ChainEpoch,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct VerifyDealsOnSectorProveCommitReturn {
    #[serde(with = "biguint_ser")]
    pub deal_weight: DealWeight,
    #[serde(with = "biguint_ser")]
    pub verified_deal_weight: DealWeight,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct ComputeDataCommitmentParams {
    pub deal_ids: Vec<DealID>,
    pub sector_type: RegisteredSealProof,
}
