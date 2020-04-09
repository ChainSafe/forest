// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::deal::ClientDealProposal;
use address::Address;
use clock::ChainEpoch;
use vm::{DealID, RegisteredProof, SectorSize, TokenAmount};

pub struct WithdrawBalanceParams {
    pub provider_or_client: Address,
    pub amount: TokenAmount,
}

pub struct OnMinerSectorsTerminateParams {
    pub deal_ids: Vec<DealID>,
}

pub struct HandleExpiredDealsParams {
    pub deal_ids: Vec<DealID>,
}

pub struct PublishStorageDealsParams {
    pub deals: Vec<ClientDealProposal>,
}

pub struct PublishStorageDealsReturn {
    pub ids: Vec<DealID>,
}

pub struct VerifyDealsOnSectorProveCommitParams {
    pub deal_ids: Vec<DealID>,
    pub sector_size: SectorSize,
    pub sector_expiry: ChainEpoch,
}

pub struct ComputeDataCommitmentParams {
    pub deal_ids: Vec<DealID>,
    pub sector_type: RegisteredProof,
}
