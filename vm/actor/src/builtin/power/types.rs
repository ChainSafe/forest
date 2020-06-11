// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::DealWeight;
use address::Address;
use clock::ChainEpoch;
use encoding::{serde_bytes, tuple::*, Cbor};
use fil_types::{RegisteredProof, SectorSize};
use num_bigint::biguint_ser;
use vm::{Serialized, TokenAmount};

pub type SectorTermination = i64;

/// Implicit termination after all deals expire
pub const SECTOR_TERMINATION_EXPIRED: SectorTermination = 0;
/// Unscheduled explicit termination by the miner
pub const SECTOR_TERMINATION_MANUAL: SectorTermination = 1;
/// Implicit termination due to unrecovered fault
pub const SECTOR_TERMINATION_FAULTY: SectorTermination = 3;

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct CreateMinerParams {
    owner_addr: Address,
    worker_addr: Address,
    seal_proof_type: RegisteredProof,
    #[serde(with = "serde_bytes")]
    peer_id: Vec<u8>,
}

#[derive(Clone, Serialize_tuple, Deserialize_tuple)]
pub struct SectorStorageWeightDesc {
    pub sector_size: SectorSize,
    pub duration: ChainEpoch,
    #[serde(with = "biguint_ser")]
    pub deal_weight: DealWeight,
    #[serde(with = "biguint_ser")]
    pub verified_deal_weight: DealWeight,
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
    pub prev_weight: SectorStorageWeightDesc,
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

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct OnFaultBeginParams {
    pub weights: Vec<SectorStorageWeightDesc>, // TODO: replace with power if it can be computed by miner
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct OnFaultEndParams {
    pub weights: Vec<SectorStorageWeightDesc>, // TODO: replace with power if it can be computed by miner
}
