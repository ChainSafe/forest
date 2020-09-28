// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{smooth::FilterEstimate, DealWeight};
use address::Address;
use clock::ChainEpoch;
use encoding::{tuple::*, BytesDe, Cbor};
use fil_types::{RegisteredSealProof, SectorSize, StoragePower};
use num_bigint::bigint_ser;
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
    pub owner: Address,
    pub worker: Address,
    pub seal_proof_type: RegisteredSealProof,
    pub peer: BytesDe,
    pub multiaddrs: Vec<BytesDe>,
}
impl Cbor for CreateMinerParams {}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct CreateMinerReturn {
    /// Canonical ID-based address for the actor.
    pub id_address: Address,
    /// Re-org safe address for created actor.
    pub robust_address: Address,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct UpdateClaimedPowerParams {
    #[serde(with = "bigint_ser")]
    pub raw_byte_delta: StoragePower,
    #[serde(with = "bigint_ser")]
    pub quality_adjusted_delta: StoragePower,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct EnrollCronEventParams {
    pub event_epoch: ChainEpoch,
    pub payload: Serialized,
}

#[derive(Clone, Serialize_tuple, Deserialize_tuple)]
pub struct SectorStorageWeightDesc {
    pub sector_size: SectorSize,
    pub duration: ChainEpoch,
    #[serde(with = "bigint_ser")]
    pub deal_weight: DealWeight,
    #[serde(with = "bigint_ser")]
    pub verified_deal_weight: DealWeight,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct ReportConsensusFaultParams {
    pub block_header_1: Serialized,
    pub block_header_2: Serialized,
    pub block_header_extra: Serialized,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct CurrentTotalPowerReturn {
    #[serde(with = "bigint_ser")]
    pub raw_byte_power: StoragePower,
    #[serde(with = "bigint_ser")]
    pub quality_adj_power: StoragePower,
    #[serde(with = "bigint_ser")]
    pub pledge_collateral: TokenAmount,
    pub quality_adj_power_smoothed: FilterEstimate,
}
