// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::DealWeight;
use address::Address;
use clock::ChainEpoch;
use encoding::{tuple::*, Cbor};
use fil_types::SectorSize;
use num_bigint::bigint_ser;
use num_bigint::biguint_ser;
use vm::{Serialized, TokenAmount};

pub type SectorTermination = i64;

/// Implicit termination after all deals expire
pub const SECTOR_TERMINATION_EXPIRED: SectorTermination = 0;
/// Unscheduled explicit termination by the miner
pub const SECTOR_TERMINATION_MANUAL: SectorTermination = 1;

#[derive(Clone, Serialize_tuple, Deserialize_tuple)]
pub struct SectorStorageWeightDesc {
    pub sector_size: SectorSize,
    pub duration: ChainEpoch,
    #[serde(with = "bigint_ser")]
    pub deal_weight: DealWeight,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct AddBalanceParams {
    pub miner: Address,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct WithdrawBalanceParams {
    pub miner: Address,
    #[serde(with = "biguint_ser")]
    pub requested: TokenAmount,
}

// TODO on miner impl, alias these params for constructor
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct CreateMinerParams {
    pub owner_addr: Address,
    pub worker_addr: Address,
    pub sector_size: SectorSize,
    pub peer: String,
}

impl Cbor for CreateMinerParams {}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct CreateMinerReturn {
    /// Canonical ID-based address for the actor.
    pub id_address: Address,
    /// Re-org safe address for created actor
    pub robust_address: Address,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct DeleteMinerParams {
    pub miner: Address,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct OnSectorProveCommitParams {
    pub weight: SectorStorageWeightDesc,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct OnSectorTerminateParams {
    pub termination_type: SectorTermination,
    pub weights: Vec<SectorStorageWeightDesc>,
    #[serde(with = "biguint_ser")]
    pub pledge: TokenAmount,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct OnSectorTemporaryFaultEffectiveBeginParams {
    // TODO revisit todo for replacing with power
    pub weights: Vec<SectorStorageWeightDesc>,
    #[serde(with = "biguint_ser")]
    pub pledge: TokenAmount,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct OnSectorTemporaryFaultEffectiveEndParams {
    // TODO revisit todo for replacing with power
    pub weights: Vec<SectorStorageWeightDesc>,
    #[serde(with = "biguint_ser")]
    pub pledge: TokenAmount,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct OnSectorModifyWeightDescParams {
    // TODO revisit todo in spec to change with power
    pub prev_weight: SectorStorageWeightDesc,
    #[serde(with = "biguint_ser")]
    pub prev_pledge: TokenAmount,
    pub new_weight: SectorStorageWeightDesc,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct OnMinerWindowedPoStFailureParams {
    pub num_consecutive_failures: i64,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct EnrollCronEventParams {
    pub event_epoch: ChainEpoch,
    pub payload: Serialized,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct ReportConsensusFaultParams {
    pub block_header_1: Serialized,
    pub block_header_2: Serialized,
    pub block_header_extra: Serialized,
}
