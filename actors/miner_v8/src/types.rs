// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;
use fil_actors_runtime_v8::DealWeight;
use fvm_ipld_bitfield::UnvalidatedBitField;
use fvm_ipld_encoding::tuple::*;
use fvm_ipld_encoding::{serde_bytes, BytesDe, Cbor};
use fvm_shared::address::Address;
use fvm_shared::bigint::bigint_ser;
use fvm_shared::clock::ChainEpoch;
use fvm_shared::deal::DealID;
use fvm_shared::econ::TokenAmount;
use fvm_shared::randomness::Randomness;
use fvm_shared::sector::{
    PoStProof, RegisteredPoStProof, RegisteredSealProof, RegisteredUpdateProof, SectorNumber,
    StoragePower,
};
use fvm_shared::smooth::FilterEstimate;

pub type CronEvent = i64;

pub const CRON_EVENT_WORKER_KEY_CHANGE: CronEvent = 0;
pub const CRON_EVENT_PROVING_DEADLINE: CronEvent = 1;
pub const CRON_EVENT_PROCESS_EARLY_TERMINATIONS: CronEvent = 2;

/// Storage miner actor constructor params are defined here so the power actor can send them to the init actor
/// to instantiate miners.
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct MinerConstructorParams {
    pub owner: Address,
    pub worker: Address,
    pub control_addresses: Vec<Address>,
    pub window_post_proof_type: RegisteredPoStProof,
    #[serde(with = "serde_bytes")]
    pub peer_id: Vec<u8>,
    pub multi_addresses: Vec<BytesDe>,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct CronEventPayload {
    pub event_type: i64,
}

#[derive(Debug)]
pub struct PartitionKey {
    pub deadline: u64,
    pub partition: u64,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct GetControlAddressesReturn {
    pub owner: Address,
    pub worker: Address,
    pub control_addresses: Vec<Address>,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct ChangeWorkerAddressParams {
    pub new_worker: Address,
    pub new_control_addresses: Vec<Address>,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct ChangePeerIDParams {
    #[serde(with = "serde_bytes")]
    pub new_id: Vec<u8>,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct ChangeMultiaddrsParams {
    pub new_multi_addrs: Vec<BytesDe>,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct ConfirmSectorProofsParams {
    pub sectors: Vec<SectorNumber>,
    pub reward_smoothed: FilterEstimate,
    #[serde(with = "bigint_ser")]
    pub reward_baseline_power: StoragePower,
    pub quality_adj_power_smoothed: FilterEstimate,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct DeferredCronEventParams {
    #[serde(with = "serde_bytes")]
    pub event_payload: Vec<u8>,
    pub reward_smoothed: FilterEstimate,
    pub quality_adj_power_smoothed: FilterEstimate,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct PoStPartition {
    /// Partitions are numbered per-deadline, from zero.
    pub index: u64,
    /// Sectors skipped while proving that weren't already declared faulty.
    pub skipped: UnvalidatedBitField,
}

/// Information submitted by a miner to provide a Window PoSt.
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct SubmitWindowedPoStParams {
    /// The deadline index which the submission targets.
    pub deadline: u64,
    /// The partitions being proven.
    pub partitions: Vec<PoStPartition>,
    /// Array of proofs, one per distinct registered proof type present in the sectors being proven.
    /// In the usual case of a single proof type, this array will always have a single element (independent of number of partitions).
    pub proofs: Vec<PoStProof>,
    /// The epoch at which these proofs is being committed to a particular chain.
    pub chain_commit_epoch: ChainEpoch,
    /// The ticket randomness on the chain at the `chain_commit_epoch` on the chain this post is committed to.
    pub chain_commit_rand: Randomness,
}

impl Cbor for SubmitWindowedPoStParams {}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct ProveCommitSectorParams {
    pub sector_number: SectorNumber,
    #[serde(with = "serde_bytes")]
    pub proof: Vec<u8>,
}

impl Cbor for ProveCommitSectorParams {}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct CheckSectorProvenParams {
    pub sector_number: SectorNumber,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct ExtendSectorExpirationParams {
    pub extensions: Vec<ExpirationExtension>,
}

impl Cbor for ExtendSectorExpirationParams {}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct ExpirationExtension {
    pub deadline: u64,
    pub partition: u64,
    pub sectors: UnvalidatedBitField,
    pub new_expiration: ChainEpoch,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct TerminateSectorsParams {
    pub terminations: Vec<TerminationDeclaration>,
}

impl Cbor for TerminateSectorsParams {}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct TerminationDeclaration {
    pub deadline: u64,
    pub partition: u64,
    pub sectors: UnvalidatedBitField,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct TerminateSectorsReturn {
    // Set to true if all early termination work has been completed. When
    // false, the miner may choose to repeatedly invoke TerminateSectors
    // with no new sectors to process the remainder of the pending
    // terminations. While pending terminations are outstanding, the miner
    // will not be able to withdraw funds.
    pub done: bool,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct DeclareFaultsParams {
    pub faults: Vec<FaultDeclaration>,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct FaultDeclaration {
    /// The deadline to which the faulty sectors are assigned, in range [0..WPoStPeriodDeadlines)
    pub deadline: u64,
    /// Partition index within the deadline containing the faulty sectors.
    pub partition: u64,
    /// Sectors in the partition being declared faulty.
    pub sectors: UnvalidatedBitField,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct DeclareFaultsRecoveredParams {
    pub recoveries: Vec<RecoveryDeclaration>,
}

impl Cbor for DeclareFaultsRecoveredParams {}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct RecoveryDeclaration {
    /// The deadline to which the recovered sectors are assigned, in range [0..WPoStPeriodDeadlines)
    pub deadline: u64,
    /// Partition index within the deadline containing the recovered sectors.
    pub partition: u64,
    /// Sectors in the partition being declared recovered.
    pub sectors: UnvalidatedBitField,
}

impl Cbor for RecoveryDeclaration {}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct CompactPartitionsParams {
    pub deadline: u64,
    pub partitions: UnvalidatedBitField,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct CompactSectorNumbersParams {
    pub mask_sector_numbers: UnvalidatedBitField,
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
    pub amount_requested: TokenAmount,
}

impl Cbor for WithdrawBalanceParams {}

#[derive(Serialize_tuple, Deserialize_tuple)]
#[serde(transparent)]
pub struct WithdrawBalanceReturn {
    pub amount_withdrawn: TokenAmount,
}

#[derive(Debug, PartialEq, Serialize_tuple, Deserialize_tuple)]
pub struct WorkerKeyChange {
    /// Must be an ID address
    pub new_worker: Address,
    pub effective_at: ChainEpoch,
}

pub type PreCommitSectorParams = SectorPreCommitInfo;

impl Cbor for PreCommitSectorParams {}

#[derive(Debug, PartialEq, Clone, Serialize_tuple, Deserialize_tuple)]
pub struct PreCommitSectorBatchParams {
    pub sectors: Vec<SectorPreCommitInfo>,
}

impl Cbor for PreCommitSectorBatchParams {}

#[derive(Debug, Default, PartialEq, Clone, Serialize_tuple, Deserialize_tuple)]
pub struct SectorPreCommitInfo {
    pub seal_proof: RegisteredSealProof,
    pub sector_number: SectorNumber,
    /// CommR
    pub sealed_cid: Cid,
    pub seal_rand_epoch: ChainEpoch,
    pub deal_ids: Vec<DealID>,
    pub expiration: ChainEpoch,
    /// Whether to replace a "committed capacity" no-deal sector (requires non-empty DealIDs)
    pub replace_capacity: bool,
    /// The committed capacity sector to replace, and its deadline/partition location
    pub replace_sector_deadline: u64,
    pub replace_sector_partition: u64,
    pub replace_sector_number: SectorNumber,
}

/// Information stored on-chain for a pre-committed sector.
#[derive(Debug, PartialEq, Clone, Serialize_tuple, Deserialize_tuple)]
pub struct SectorPreCommitOnChainInfo {
    pub info: SectorPreCommitInfo,
    pub pre_commit_deposit: TokenAmount,
    pub pre_commit_epoch: ChainEpoch,
    /// Integral of active deals over sector lifetime, 0 if CommittedCapacity sector
    #[serde(with = "bigint_ser")]
    pub deal_weight: DealWeight,
    /// Integral of active verified deals over sector lifetime
    #[serde(with = "bigint_ser")]
    pub verified_deal_weight: DealWeight,
}

/// Information stored on-chain for a proven sector.
#[derive(Debug, Default, PartialEq, Clone, Serialize_tuple, Deserialize_tuple)]
pub struct SectorOnChainInfo {
    pub sector_number: SectorNumber,
    /// The seal proof type implies the PoSt proofs
    pub seal_proof: RegisteredSealProof,
    /// CommR
    pub sealed_cid: Cid,
    pub deal_ids: Vec<DealID>,
    /// Epoch during which the sector proof was accepted
    pub activation: ChainEpoch,
    /// Epoch during which the sector expires
    pub expiration: ChainEpoch,
    /// Integral of active deals over sector lifetime
    #[serde(with = "bigint_ser")]
    pub deal_weight: DealWeight,
    /// Integral of active verified deals over sector lifetime
    #[serde(with = "bigint_ser")]
    pub verified_deal_weight: DealWeight,
    /// Pledge collected to commit this sector
    pub initial_pledge: TokenAmount,
    /// Expected one day projection of reward for sector computed at activation time
    pub expected_day_reward: TokenAmount,
    /// Expected twenty day projection of reward for sector computed at activation time
    pub expected_storage_pledge: TokenAmount,
    /// Age of sector this sector replaced or zero
    pub replaced_sector_age: ChainEpoch,
    /// Day reward of sector this sector replace or zero
    pub replaced_day_reward: TokenAmount,
    /// The original SealedSectorCID, only gets set on the first ReplicaUpdate
    pub sector_key_cid: Option<Cid>,
}

#[derive(Debug, PartialEq, Copy, Clone, Serialize_tuple, Deserialize_tuple)]
pub struct Fault {
    pub miner: Address,
    pub fault: ChainEpoch,
}

// * Added in v2 -- param was previously a big int.
#[derive(Debug, Serialize_tuple, Deserialize_tuple)]
pub struct ApplyRewardParams {
    pub reward: TokenAmount,
    pub penalty: TokenAmount,
}

impl Cbor for DisputeWindowedPoStParams {}

#[derive(Debug, PartialEq, Clone, Copy, Serialize_tuple, Deserialize_tuple)]
pub struct DisputeWindowedPoStParams {
    pub deadline: u64,
    pub post_index: u64, // only one is allowed at a time to avoid loading too many sector infos.
}

impl Cbor for ProveCommitAggregateParams {}

#[derive(Debug, Serialize_tuple, Deserialize_tuple)]
pub struct ProveCommitAggregateParams {
    pub sector_numbers: UnvalidatedBitField,
    #[serde(with = "serde_bytes")]
    pub aggregate_proof: Vec<u8>,
}

#[derive(Debug, PartialEq, Serialize_tuple, Deserialize_tuple)]
pub struct ReplicaUpdate {
    pub sector_number: SectorNumber,
    pub deadline: u64,
    pub partition: u64,
    pub new_sealed_cid: Cid,
    pub deals: Vec<DealID>,
    pub update_proof_type: RegisteredUpdateProof,
    #[serde(with = "serde_bytes")]
    pub replica_proof: Vec<u8>,
}

#[derive(Debug, Serialize_tuple, Deserialize_tuple)]
pub struct ProveReplicaUpdatesParams {
    pub updates: Vec<ReplicaUpdate>,
}

impl Cbor for ProveReplicaUpdatesParams {}
