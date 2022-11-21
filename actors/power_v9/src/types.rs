// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fvm_ipld_encoding::tuple::*;
use fvm_ipld_encoding::{serde_bytes, BytesDe, Cbor, RawBytes};
use fvm_shared::address::Address;
use fvm_shared::bigint::bigint_ser;
use fvm_shared::clock::ChainEpoch;
use fvm_shared::econ::TokenAmount;
use fvm_shared::sector::{RegisteredPoStProof, StoragePower};
use fvm_shared::smooth::FilterEstimate;

pub type SectorTermination = i64;

/// Implicit termination after all deals expire
pub const SECTOR_TERMINATION_EXPIRED: SectorTermination = 0;
/// Unscheduled explicit termination by the miner
pub const SECTOR_TERMINATION_MANUAL: SectorTermination = 1;
/// Implicit termination due to unrecovered fault
pub const SECTOR_TERMINATION_FAULTY: SectorTermination = 3;

pub const CRON_QUEUE_HAMT_BITWIDTH: u32 = 6;
pub const CRON_QUEUE_AMT_BITWIDTH: u32 = 6;
pub const PROOF_VALIDATION_BATCH_AMT_BITWIDTH: u32 = 4;

#[derive(Serialize_tuple, Deserialize_tuple, Clone)]
pub struct CreateMinerParams {
    pub owner: Address,
    pub worker: Address,
    pub window_post_proof_type: RegisteredPoStProof,
    #[serde(with = "serde_bytes")]
    pub peer: Vec<u8>,
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
    pub payload: RawBytes,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct CurrentTotalPowerReturn {
    #[serde(with = "bigint_ser")]
    pub raw_byte_power: StoragePower,
    #[serde(with = "bigint_ser")]
    pub quality_adj_power: StoragePower,
    pub pledge_collateral: TokenAmount,
    pub quality_adj_power_smoothed: FilterEstimate,
}
