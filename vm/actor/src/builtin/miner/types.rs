// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::DealWeight;
use address::Address;
use bitfield::BitField;
use cid::Cid;
use clock::ChainEpoch;
use encoding::{serde_bytes, tuple::*};
use fil_types::{PoStProof, RegisteredSealProof, SectorNumber};
use num_bigint::bigint_ser;
use vm::{DealID, TokenAmount};

pub type CronEvent = i64;
pub const CRON_EVENT_WORKER_KEY_CHANGE: CronEvent = 1;
pub const CRON_EVENT_PRE_COMMIT_EXPIRY: CronEvent = 2;
pub const CRON_EVENT_PROVING_PERIOD: CronEvent = 3;

/// Storage miner actor constructor params are defined here so the power actor can send them to the init actor
/// to instantiate miners.
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct MinerConstructorParams {
    pub owner: Address,
    pub worker: Address,
    pub seal_proof_type: RegisteredSealProof,
    #[serde(with = "serde_bytes")]
    pub peer_id: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub multi_address: Vec<u8>,
}
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct CronEventPayload {
    pub event_type: i64,
    pub sectors: Option<BitField>,
}
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct GetControlAddressesReturn {
    pub owner: Address,
    pub worker: Address,
}
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct ChangeWorkerAddressParams {
    pub new_worker: Address,
}
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct ChangePeerIDParams {
    #[serde(with = "serde_bytes")]
    pub new_id: Vec<u8>,
}
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct ChangeMultiaddrsParams {
    #[serde(with = "serde_bytes")]
    pub new_multi_addrs: Vec<u8>,
}
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct ConfirmSectorProofsParams {
    pub sectors: Vec<SectorNumber>,
}
/// Information submitted by a miner to provide a Window PoSt.
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct SubmitWindowedPoStParams {
    /// The deadline index which the submission targets.
    pub deadline: u64,
    /// The partition indices being proven.
    /// Partitions are counted across all deadlines, such that all partition indices in the second deadline are greater
    /// than the partition numbers in the first deadlines.
    pub partitions: Vec<u64>,
    /// Array of proofs, one per distinct registered proof type present in the sectors being proven.
    /// In the usual case of a single proof type, this array will always have a single element (independent of number of partitions).
    pub proofs: Vec<PoStProof>,
    /// Sectors skipped while proving that weren't already declared faulty
    pub skipped: BitField,
}
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct ProveCommitSectorParams {
    pub sector_number: SectorNumber,
    #[serde(with = "serde_bytes")]
    pub proof: Vec<u8>,
}
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct CheckSectorProvenParams {
    pub sector_number: SectorNumber,
}
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct ExtendSectorExpirationParams {
    pub sector_number: SectorNumber,
    pub new_expiration: ChainEpoch,
}
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct TerminateSectorsParams {
    pub sectors: BitField,
}
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct DeclareFaultsParams {
    pub faults: Vec<FaultDeclaration>,
}
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct FaultDeclaration {
    pub deadline: u64, // In range [0..WPoStPeriodDeadlines)
    pub sectors: BitField,
}
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct DeclareFaultsRecoveredParams {
    pub recoveries: Vec<RecoveryDeclaration>,
}
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct RecoveryDeclaration {
    pub deadline: u64, // In range [0..WPoStPeriodDeadlines)
    pub sectors: BitField,
}
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct ReportConsensusFaultParams {
    #[serde(with = "serde_bytes")]
    pub header1: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub header2: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub header_extra: Vec<u8>,
}
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct WithdrawBalanceParams {
    #[serde(with = "bigint_ser")]
    pub amount_requested: TokenAmount,
}
#[derive(Debug, PartialEq, Serialize_tuple, Deserialize_tuple)]
pub struct WorkerKeyChange {
    /// Must be an ID address
    pub new_worker: Address,
    pub effective_at: ChainEpoch,
}
#[derive(Debug, PartialEq, Clone, Serialize_tuple, Deserialize_tuple)]
pub struct SectorPreCommitInfo {
    pub registered_proof: RegisteredSealProof,
    pub sector_number: SectorNumber,
    /// CommR
    pub sealed_cid: Cid,
    pub seal_rand_epoch: ChainEpoch,
    pub deal_ids: Vec<DealID>,
    /// Sector Expiration
    pub expiration: ChainEpoch,
}
#[derive(Debug, PartialEq, Clone, Serialize_tuple, Deserialize_tuple)]
pub struct SectorPreCommitOnChainInfo {
    pub info: SectorPreCommitInfo,
    #[serde(with = "bigint_ser")]
    pub pre_commit_deposit: TokenAmount,
    pub pre_commit_epoch: ChainEpoch,
}
#[derive(Debug, PartialEq, Clone, Serialize_tuple, Deserialize_tuple)]
pub struct SectorOnChainInfo {
    pub info: SectorPreCommitInfo,
    /// Epoch at which SectorProveCommit is accepted
    pub activation_epoch: ChainEpoch,
    /// Integral of active deals over sector lifetime, 0 if CommittedCapacity sector
    #[serde(with = "bigint_ser")]
    pub deal_weight: DealWeight,
    /// Integral of active verified deals over sector lifetime
    #[serde(with = "bigint_ser")]
    pub verified_deal_weight: DealWeight,
}

#[derive(Debug, PartialEq, Clone, Serialize_tuple, Deserialize_tuple)]
pub struct ChainSectorInfo {
    pub info: SectorPreCommitInfo,
    pub id: SectorNumber,
}

#[derive(Debug, PartialEq, Clone, Serialize_tuple, Deserialize_tuple)]
pub struct Fault {
    pub miner: Address,
    pub fault: ChainEpoch,
}
