// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::{
    address::Address,
    clock::ChainEpoch,
    econ::TokenAmount,
    sector::{PoStProof, RegisteredPoStProof, RegisteredSealProof, SectorNumber},
};
use ::cid::Cid;
use fil_actors_shared::fvm_ipld_bitfield::{BitField, UnvalidatedBitField};
use fil_actors_shared::v16::reward::FilterEstimate;
use fvm_ipld_encoding::{BytesDe, RawBytes};
use fvm_shared4::deal::DealID;
use fvm_shared4::sector::RegisteredUpdateProof;
use itertools::Itertools as _;
use num::BigInt;
use pastey::paste;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ConstructorParamsLotusJson {
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub owner_addr: Address,
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub worker_addr: Address,
    #[schemars(with = "LotusJson<Vec<Address>>")]
    #[serde(with = "crate::lotus_json")]
    pub control_addrs: Vec<Address>,
    #[schemars(with = "LotusJson<RegisteredPoStProof>")]
    #[serde(with = "crate::lotus_json")]
    pub window_po_st_proof_type: RegisteredPoStProof,
    #[schemars(with = "LotusJson<Vec<u8>>")]
    #[serde(with = "crate::lotus_json")]
    pub peer_id: Vec<u8>,
    #[schemars(with = "LotusJson<Vec<Vec<u8>>>")]
    #[serde(with = "crate::lotus_json")]
    pub multiaddrs: Vec<Vec<u8>>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ChangeWorkerAddressParamsLotusJson {
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub new_worker: Address,

    #[schemars(with = "LotusJson<Vec<Address>>")]
    #[serde(with = "crate::lotus_json")]
    #[serde(rename = "NewControlAddrs")]
    pub new_control_addresses: Vec<Address>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ChangePeerIDParamsLotusJson {
    #[schemars(with = "LotusJson<Vec<u8>>")]
    #[serde(with = "crate::lotus_json", rename = "NewID")]
    pub new_id: Vec<u8>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ChangeMultiaddrsParamsLotusJson {
    #[schemars(with = "LotusJson<Vec<Vec<u8>>>")]
    #[serde(with = "crate::lotus_json", rename = "NewMultiaddrs")]
    pub new_multi_addrs: Vec<Vec<u8>>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct PoStPartitionLotusJson {
    pub index: u64,
    #[schemars(with = "LotusJson<BitField>")]
    #[serde(with = "crate::lotus_json")]
    pub skipped: BitField,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct SubmitWindowedPoStParamsLotusJson {
    pub deadline: u64,
    pub partitions: Vec<PoStPartitionLotusJson>,
    #[schemars(with = "LotusJson<Vec<PoStProof>>")]
    #[serde(with = "crate::lotus_json")]
    pub proofs: Vec<PoStProof>,
    pub chain_commit_epoch: ChainEpoch,
    #[schemars(with = "LotusJson<Vec<u8>>")]
    #[serde(with = "crate::lotus_json")]
    pub chain_commit_rand: Vec<u8>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct TerminationDeclarationLotusJson {
    pub deadline: u64,
    pub partition: u64,
    #[schemars(with = "LotusJson<BitField>")]
    #[serde(with = "crate::lotus_json")]
    pub sectors: BitField,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct TerminateSectorsParamsLotusJson {
    pub terminations: Vec<TerminationDeclarationLotusJson>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct FaultDeclarationLotusJson {
    pub deadline: u64,
    pub partition: u64,
    #[schemars(with = "LotusJson<BitField>")]
    #[serde(with = "crate::lotus_json")]
    pub sectors: BitField,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct DeclareFaultsParamsLotusJson {
    pub faults: Vec<FaultDeclarationLotusJson>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct RecoveryDeclarationLotusJson {
    pub deadline: u64,
    pub partition: u64,
    #[schemars(with = "LotusJson<BitField>")]
    #[serde(with = "crate::lotus_json")]
    pub sectors: BitField,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct DeclareFaultsRecoveredParamsLotusJson {
    pub recoveries: Vec<RecoveryDeclarationLotusJson>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct WithdrawBalanceParamsLotusJson {
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub amount_requested: TokenAmount,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ChangeBeneficiaryParamsLotusJson {
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub new_beneficiary: Address,
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub new_quota: TokenAmount,
    pub new_expiration: ChainEpoch,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ChangeOwnerAddressParamsLotusJson(
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    Address,
);

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct CompactPartitionsParamsLotusJson {
    pub deadline: u64,
    #[schemars(with = "LotusJson<BitField>")]
    #[serde(with = "crate::lotus_json")]
    pub partitions: BitField,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct CompactSectorNumbersParamsLotusJson {
    #[schemars(with = "LotusJson<BitField>")]
    #[serde(with = "crate::lotus_json")]
    pub mask_sector_numbers: BitField,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct DisputeWindowedPoStParamsLotusJson {
    pub deadline: u64,
    #[serde(rename = "PoStIndex")]
    pub post_index: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ExtendSectorExpirationParamsV8LotusJson {
    pub extensions: Vec<ExpirationExtensionV8LotusJson>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ExtendSectorExpirationParamsLotusJson {
    pub extensions: Vec<ExpirationExtensionLotusJson>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ExpirationExtensionV8LotusJson {
    pub deadline: u64,
    pub partition: u64,
    #[schemars(with = "LotusJson<Vec<u8>>")]
    #[serde(with = "crate::lotus_json")]
    pub sectors: Vec<u8>,
    pub new_expiration: ChainEpoch,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ExpirationExtensionLotusJson {
    pub deadline: u64,
    pub partition: u64,
    #[schemars(with = "LotusJson<BitField>")]
    #[serde(with = "crate::lotus_json")]
    pub sectors: BitField,
    pub new_expiration: ChainEpoch,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ExtendSectorExpiration2ParamsLotusJson {
    pub extensions: Vec<ExpirationExtension2LotusJson>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ExpirationExtension2LotusJson {
    pub deadline: u64,
    pub partition: u64,
    #[schemars(with = "LotusJson<BitField>")]
    #[serde(with = "crate::lotus_json")]
    pub sectors: BitField,
    // Lotus returns null (not []) when there are no sector claims; model as Option, so None means empty.
    pub sectors_with_claims: Option<Vec<SectorClaimLotusJson>>,
    pub new_expiration: ChainEpoch,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct SectorClaimLotusJson {
    pub sector_number: SectorNumber,
    pub maintain_claims: Vec<u64>,
    pub drop_claims: Vec<u64>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct SectorPreCommitInfoLotusJson {
    #[schemars(with = "LotusJson<RegisteredSealProof>")]
    #[serde(with = "crate::lotus_json")]
    pub seal_proof: RegisteredSealProof,

    pub sector_number: SectorNumber,

    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json", rename = "SealedCID")]
    pub sealed_cid: Cid,

    pub seal_rand_epoch: ChainEpoch,

    // Lotus returns null (not []) when there are no sector claims; model as Option, so None means empty.
    #[serde(rename = "DealIDs")]
    pub deal_ids: Option<Vec<u64>>,

    pub expiration: ChainEpoch,
    #[schemars(with = "LotusJson<Option<Cid>>")]
    #[serde(with = "crate::lotus_json")]
    pub unsealed_cid: Option<Cid>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct PreCommitSectorParamsLotusJson {
    #[schemars(with = "LotusJson<RegisteredSealProof>")]
    #[serde(with = "crate::lotus_json")]
    pub seal_proof: RegisteredSealProof,
    pub sector_number: SectorNumber,
    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json", rename = "SealedCID")]
    pub sealed_cid: Cid,
    pub seal_rand_epoch: ChainEpoch,
    #[serde(rename = "DealIDs")]
    pub deal_ids: Vec<u64>,
    pub expiration: ChainEpoch,
    pub replace_capacity: bool,
    pub replace_sector_deadline: u64,
    pub replace_sector_partition: u64,
    pub replace_sector_number: fvm_shared2::sector::SectorNumber,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct PreCommitSectorBatchParamsLotusJson {
    pub sectors: Vec<PreCommitSectorParamsLotusJson>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct PreCommitSectorBatch2ParamsLotusJson {
    pub sectors: Vec<SectorPreCommitInfoLotusJson>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct SectorActivationManifestLotusJson {
    pub sector_number: SectorNumber,
    pub pieces: Vec<PieceActivationManifestLotusJson>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct PieceActivationManifestLotusJson {
    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json", rename = "CID")]
    pub cid: Cid,
    pub size: u64,
    pub verified_allocation_key: Option<VerifiedAllocationKeyLotusJson>,
    // Lotus returns null (not []) when there are no sector claims; model as Option, so None means empty.
    pub notify: Option<Vec<DataActivationNotificationLotusJson>>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct VerifiedAllocationKeyLotusJson {
    pub client: u64,
    pub id: u64,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct DataActivationNotificationLotusJson {
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub address: Address,
    #[schemars(with = "LotusJson<RawBytes>")]
    #[serde(with = "crate::lotus_json")]
    pub payload: RawBytes,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ProveCommitSectorParamsLotusJson {
    pub sector_number: SectorNumber,
    #[schemars(with = "LotusJson<Vec<u8>>")]
    #[serde(with = "crate::lotus_json")]
    pub proof: Vec<u8>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ProveCommitSectors3ParamsLotusJson {
    pub sector_activations: Vec<SectorActivationManifestLotusJson>,
    #[schemars(with = "LotusJson<Vec<RawBytes>>")]
    #[serde(with = "crate::lotus_json")]
    pub sector_proofs: Vec<RawBytes>,
    #[schemars(with = "LotusJson<RawBytes>")]
    #[serde(with = "crate::lotus_json")]
    pub aggregate_proof: RawBytes,
    pub aggregate_proof_type: Option<i64>,
    pub require_activation_success: bool,
    pub require_notification_success: bool,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct SectorUpdateManifestLotusJson {
    pub sector: SectorNumber,
    pub deadline: u64,
    pub partition: u64,
    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json", rename = "NewSealedCID")]
    pub new_sealed_cid: Cid,
    pub pieces: Vec<PieceActivationManifestLotusJson>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ProveReplicaUpdates3ParamsLotusJson {
    pub sector_updates: Vec<SectorUpdateManifestLotusJson>,

    #[schemars(with = "LotusJson<Vec<RawBytes>>")]
    #[serde(with = "crate::lotus_json")]
    pub sector_proofs: Vec<RawBytes>,

    #[schemars(with = "LotusJson<RawBytes>")]
    #[serde(with = "crate::lotus_json")]
    pub aggregate_proof: RawBytes,

    pub update_proofs_type: i64,
    pub aggregate_proof_type: Option<i64>,
    pub require_activation_success: bool,
    pub require_notification_success: bool,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ReportConsensusFaultParamsLotusJson {
    #[schemars(with = "LotusJson<Vec<u8>>")]
    #[serde(with = "crate::lotus_json", rename = "BlockHeader1")]
    pub header1: Vec<u8>,
    #[schemars(with = "LotusJson<Vec<u8>>")]
    #[serde(with = "crate::lotus_json", rename = "BlockHeader2")]
    pub header2: Vec<u8>,
    #[schemars(with = "LotusJson<Vec<u8>>")]
    #[serde(with = "crate::lotus_json", rename = "BlockHeaderExtra")]
    pub header_extra: Vec<u8>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct CheckSectorProvenParamsLotusJson {
    pub sector_number: SectorNumber,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ApplyRewardParamsLotusJson {
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub reward: TokenAmount,
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub penalty: TokenAmount,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ProveCommitAggregateParamsLotusJson {
    #[schemars(with = "LotusJson<BitField>")]
    #[serde(with = "crate::lotus_json")]
    pub sector_numbers: BitField,
    #[schemars(with = "LotusJson<RawBytes>")]
    #[serde(with = "crate::lotus_json")]
    pub aggregate_proof: RawBytes,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ReplicaUpdateLotusJson {
    #[serde(rename = "SectorID")]
    pub sector_number: SectorNumber,
    pub deadline: u64,
    pub partition: u64,
    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json", rename = "NewSealedSectorCID")]
    pub new_sealed_cid: Cid,

    // Lotus returns null (not []) when there are no sector claims; model as Option, so None means empty.
    #[serde(rename = "Deals")]
    pub deals: Option<Vec<u64>>,
    pub update_proof_type: i64,
    #[schemars(with = "LotusJson<RawBytes>")]
    #[serde(with = "crate::lotus_json")]
    pub replica_proof: RawBytes,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ProveReplicaUpdatesParamsLotusJson {
    pub updates: Vec<ReplicaUpdateLotusJson>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct IsControllingAddressParamLotusJson(
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    Address,
);

#[derive(Serialize, Deserialize, JsonSchema, Debug, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ConfirmSectorProofsParamsLotusJson {
    pub sector_numbers: Vec<SectorNumber>,

    #[schemars(with = "LotusJson<FilterEstimate>")]
    #[serde(with = "crate::lotus_json")]
    pub reward_smoothed: FilterEstimate,

    #[schemars(with = "LotusJson<BigInt>")]
    #[serde(with = "crate::lotus_json")]
    pub reward_baseline_power: BigInt,

    #[schemars(with = "LotusJson<FilterEstimate>")]
    #[serde(with = "crate::lotus_json")]
    pub quality_adj_power_smoothed: FilterEstimate,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct DeferredCronEventParamsLotusJson {
    #[schemars(with = "LotusJson<Vec<u8>>")]
    #[serde(with = "crate::lotus_json")]
    pub event_payload: Vec<u8>,

    #[schemars(with = "LotusJson<FilterEstimate>")]
    #[serde(with = "crate::lotus_json")]
    pub reward_smoothed: FilterEstimate,

    #[schemars(with = "LotusJson<FilterEstimate>")]
    #[serde(with = "crate::lotus_json")]
    pub quality_adj_power_smoothed: FilterEstimate,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct MaxTerminationFeeParamsLotusJson {
    #[schemars(with = "LotusJson<BigInt>")]
    #[serde(with = "crate::lotus_json")]
    pub power: BigInt,
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub initial_pledge: TokenAmount,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ReplicaUpdate2LotusJson {
    pub sector_number: SectorNumber,
    pub deadline: u64,
    pub partition: u64,

    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    pub new_sealed_cid: Cid,

    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    pub new_unsealed_cid: Cid,

    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    pub deals: Vec<DealID>,

    pub update_proof_type: i64,

    #[schemars(with = "LotusJson<Vec<u8>>")]
    #[serde(with = "crate::lotus_json")]
    pub replica_proof: Vec<u8>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ProveReplicaUpdatesParams2LotusJson {
    pub updates: Vec<ReplicaUpdate2LotusJson>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct SectorNIActivationInfoLotusJson {
    pub sealing_number: SectorNumber,
    #[serde(rename = "SealerID")]
    pub sealer_id: u64,
    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json", rename = "SealedCID")]
    pub sealed_cid: Cid,
    pub sector_number: SectorNumber,
    pub seal_rand_epoch: ChainEpoch,
    pub expiration: ChainEpoch,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct InternalSectorSetupForPresealParamsLotusJson {
    pub sectors: Vec<SectorNumber>,
    #[schemars(with = "LotusJson<FilterEstimate>")]
    #[serde(with = "crate::lotus_json")]
    pub reward_smoothed: FilterEstimate,
    #[schemars(with = "LotusJson<BigInt>")]
    #[serde(with = "crate::lotus_json")]
    pub reward_baseline_power: BigInt,
    #[schemars(with = "LotusJson<FilterEstimate>")]
    #[serde(with = "crate::lotus_json")]
    pub quality_adj_power_smoothed: FilterEstimate,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ProveCommitSectorsNIParamsLotusJson {
    pub sectors: Vec<SectorNIActivationInfoLotusJson>,
    #[schemars(with = "LotusJson<RawBytes>")]
    #[serde(with = "crate::lotus_json")]
    pub aggregate_proof: RawBytes,
    #[schemars(with = "LotusJson<RegisteredSealProof>")]
    #[serde(with = "crate::lotus_json")]
    pub seal_proof_type: RegisteredSealProof,
    pub aggregate_proof_type: i64,
    pub proving_deadline: u64,
    pub require_activation_success: bool,
}

macro_rules! impl_lotus_json_for_miner_change_worker_param {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_miner_change_worker_param_ $version>] {
                    use super::*;
					type T = fil_actor_miner_state::[<v $version>]::ChangeWorkerAddressParams;
					#[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }
                    impl HasLotusJson for T {
                        type LotusJson = ChangeWorkerAddressParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![
                                (
                                    json!({
                                        "NewWorker": "f01234",
                                        "NewControlAddrs": ["f01236", "f01237"],
                                    }),
                                    Self {
                                        new_worker: Address::new_id(1234).into(),
                                        new_control_addresses: vec![Address::new_id(1236).into(), Address::new_id(1237).into()],
                                    },
                                ),
                            ]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            ChangeWorkerAddressParamsLotusJson {
                                new_worker: self.new_worker.into(),
                                new_control_addresses: self.new_control_addresses
                                    .into_iter()
                                    .map(|a| a.into())
                                    .collect(),
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                new_worker: lotus_json.new_worker.into(),
                                new_control_addresses: lotus_json.new_control_addresses
                                    .into_iter()
                                    .map(|a| a.into())
                                    .collect(),
                            }
                        }
                    }
                }
            }
        )+
    };
}

macro_rules! impl_lotus_json_for_miner_constructor_params {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_miner_constructor_params_ $version>] {
                    use super::*;
                    type T = fil_actor_miner_state::[<v $version>]::MinerConstructorParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }
                    impl HasLotusJson for T {
                        type LotusJson = ConstructorParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![
                                (
                                    json!({
                                        "OwnerAddr": "f01234",
                                        "WorkerAddr": "f01235",
                                        "ControlAddrs": ["f01236", "f01237"],
                                        "WindowPoStProofType": 10,
                                        "PeerId": "AQ==",
                                        "Multiaddrs": ["L2lwNC8xMjcuMC4wLjEvdGNwLzgwODA=", "L2Rucy9leGFtcGxlLmNvbQ=="],
                                    }),
                                    Self {
                                        owner: Address::new_id(1234).into(),
                                        worker: Address::new_id(1235).into(),
                                        control_addresses: vec![Address::new_id(1236).into(), Address::new_id(1237).into()],
                                        window_post_proof_type: RegisteredPoStProof::from(fvm_shared4::sector::RegisteredPoStProof::StackedDRGWindow2KiBV1P1).into(),
                                        peer_id: vec![1],
                                        multi_addresses: vec![
                                            BytesDe(b"/ip4/127.0.0.1/tcp/8080".to_vec()),
                                            BytesDe(b"/dns/example.com".to_vec()),
                                        ],
                                    },
                                ),
                            ]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            ConstructorParamsLotusJson {
                                owner_addr: self.owner.into(),
                                worker_addr: self.worker.into(),
                                control_addrs: self.control_addresses.into_iter().map(|a| a.into()).collect(),
                                window_po_st_proof_type: self.window_post_proof_type.into(),
                                peer_id: self.peer_id,
                                multiaddrs: self.multi_addresses.into_iter().map(|addr| addr.0).collect(),
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                owner: lotus_json.owner_addr.into(),
                                worker: lotus_json.worker_addr.into(),
                                control_addresses: lotus_json.control_addrs
                                    .into_iter()
                                    .map(|a| a.into())
                                    .collect(),
                                window_post_proof_type: lotus_json.window_po_st_proof_type.into(),
                                peer_id: lotus_json.peer_id,
                                multi_addresses: lotus_json.multiaddrs.into_iter().map(BytesDe).collect(),
                            }
                        }
                    }
                }
            }
        )+
    };
}

macro_rules! impl_lotus_json_for_miner_declare_faults_recovered_params {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_miner_declare_faults_recovered_params_ $version>] {
                    use super::*;
                    type T = fil_actor_miner_state::[<v $version>]::DeclareFaultsRecoveredParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }
                    impl HasLotusJson for T {
                        type LotusJson = DeclareFaultsRecoveredParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(
                                json!({
                                    "Recoveries": [
                                        {
                                            "Deadline": 1,
                                            "Partition": 2,
                                            "Sectors": [0]
                                        }
                                    ]
                                }),
                                Self {
                                    recoveries: vec![fil_actor_miner_state::[<v $version>]::RecoveryDeclaration {
                                        deadline: 1,
                                        partition: 2,
                                        sectors: BitField::new().into(),
                                    }],
                                },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            DeclareFaultsRecoveredParamsLotusJson {
                                recoveries: self.recoveries.into_iter().map(|r| r.into_lotus_json()).collect(),
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                recoveries: lotus_json.recoveries.into_iter().map(|r| fil_actor_miner_state::[<v $version>]::RecoveryDeclaration::from_lotus_json(r)).collect(),
                            }
                        }
                    }
                }
            }
        )+
    };
}

macro_rules! impl_lotus_json_for_recover_declaration_params_v9_and_above {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_recover_declaration_params_ $version>] {
                    use super::*;
                    type T = fil_actor_miner_state::[<v $version>]::RecoveryDeclaration;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }
                    impl HasLotusJson for T {
                        type LotusJson = RecoveryDeclarationLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            let sectors = BitField::new();
                            vec![(
                                json!({
                                    "Deadline": 1,
                                    "Partition": 2,
                                     "Sectors": [0]
                                }),
                                Self {
                                    deadline: 1,
                                    partition: 2,
                                    sectors,
                                },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            RecoveryDeclarationLotusJson {
                                deadline: self.deadline,
                                partition: self.partition,
                                sectors: self.sectors,
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                deadline: lotus_json.deadline,
                                partition: lotus_json.partition,
                                sectors: lotus_json.sectors,
                            }
                        }
                    }
                }
            }
        )+
    };
}

impl HasLotusJson for fil_actor_miner_state::v8::RecoveryDeclaration {
    type LotusJson = RecoveryDeclarationLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        let sectors = BitField::new();
        vec![(
            json!({
                "Deadline": 1,
                "Partition": 2,
                 "Sectors": [0]
            }),
            Self {
                deadline: 1,
                partition: 2,
                sectors: sectors.into(),
            },
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        RecoveryDeclarationLotusJson {
            deadline: self.deadline,
            partition: self.partition,
            sectors: self.sectors.try_into().unwrap_or_else(|_| BitField::new()),
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        Self {
            deadline: lotus_json.deadline,
            partition: lotus_json.partition,
            sectors: lotus_json.sectors.into(),
        }
    }
}

macro_rules! impl_lotus_json_for_miner_change_owner_address_params {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_miner_change_owner_address_params_ $version>] {
                    use super::*;
                    type T = fil_actor_miner_state::[<v $version>]::ChangeOwnerAddressParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }
                    impl HasLotusJson for T {
                        type LotusJson = ChangeOwnerAddressParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(
                                json!("f01234"),
                                Self {
                                    new_owner: Address::new_id(1234).into(),
                                },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            ChangeOwnerAddressParamsLotusJson(self.new_owner.into())
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self { new_owner: lotus_json.0.into() }
                        }
                    }
                }
            }
        )+
    };
}

macro_rules! impl_lotus_json_for_miner_change_beneficiary_params {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_miner_change_beneficiary_params_ $version>] {
                    use super::*;
                    type T = fil_actor_miner_state::[<v $version>]::ChangeBeneficiaryParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }
                    impl HasLotusJson for T {
                        type LotusJson = ChangeBeneficiaryParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(
                                json!({
                                    "NewBeneficiary": "f01234",
                                    "NewQuota": "1000000000000000000",
                                    "NewExpiration": 1000
                                }),
                                Self {
                                    new_beneficiary: Address::new_id(1234).into(),
                                    new_quota: TokenAmount::from_atto(1000000000000000000u64).into(),
                                    new_expiration: 1000,
                                },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            ChangeBeneficiaryParamsLotusJson {
                                new_beneficiary: self.new_beneficiary.into(),
                                new_quota: self.new_quota.into(),
                                new_expiration: self.new_expiration,
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                new_beneficiary: lotus_json.new_beneficiary.into(),
                                new_quota: lotus_json.new_quota.into(),
                                new_expiration: lotus_json.new_expiration,
                            }
                        }
                    }
                }
            }
        )+
    };
}

macro_rules! impl_lotus_json_for_miner_extend_sector_expiration2_params {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_miner_extend_sector_expiration2_params_ $version>] {
                    use super::*;
                    type T = fil_actor_miner_state::[<v $version>]::ExtendSectorExpiration2Params;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }
                    impl HasLotusJson for T {
                        type LotusJson = ExtendSectorExpiration2ParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            let sectors = BitField::new();
                            vec![(
                                json!({
                                    "Extensions": [
                                        {
                                            "Deadline": 1,
                                            "Partition": 2,
                                            "Sectors": [0],
                                            "SectorsWithClaims": [
                                                {
                                                    "SectorNumber": 1,
                                                    "MaintainClaims": [1, 2],
                                                    "DropClaims": [3, 4]
                                                }
                                            ],
                                            "NewExpiration": 1000
                                        }
                                    ]
                                }),
                                Self {
                                    extensions: vec![fil_actor_miner_state::[<v $version>]::ExpirationExtension2 {
                                        deadline: 1,
                                        partition: 2,
                                        sectors,
                                        sectors_with_claims: vec![fil_actor_miner_state::[<v $version>]::SectorClaim {
                                            sector_number: 1,
                                            maintain_claims: vec![1, 2],
                                            drop_claims: vec![3, 4],
                                        }],
                                        new_expiration: 1000,
                                    }],
                                },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            ExtendSectorExpiration2ParamsLotusJson {
                                extensions: self.extensions.into_iter().map(|e| e.into_lotus_json()).collect(),
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                extensions: lotus_json.extensions.into_iter().map(|e| fil_actor_miner_state::[<v $version>]::ExpirationExtension2::from_lotus_json(e)).collect(),
                            }
                        }
                    }
                }

                impl HasLotusJson for fil_actor_miner_state::[<v $version>]::ExpirationExtension2 {
                    type LotusJson = ExpirationExtension2LotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        let sectors = BitField::new();
                        vec![(
                            json!({
                                "Deadline": 1,
                                "Partition": 2,
                                "Sectors": [0],
                                "SectorsWithClaims": [
                                    {
                                        "SectorNumber": 1,
                                        "MaintainClaims": [1, 2],
                                        "DropClaims": [3, 4]
                                    }
                                ],
                                "NewExpiration": 1000
                            }),
                            Self {
                                deadline: 1,
                                partition: 2,
                                sectors,
                                sectors_with_claims: vec![fil_actor_miner_state::[<v $version>]::SectorClaim {
                                    sector_number: 1,
                                    maintain_claims: vec![1, 2],
                                    drop_claims: vec![3, 4],
                                }],
                                new_expiration: 1000,
                            },
                        )]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        ExpirationExtension2LotusJson {
                            deadline: self.deadline,
                            partition: self.partition,
                            sectors: self.sectors.clone(),
                            sectors_with_claims: if self.sectors_with_claims.is_empty() {
                                None
                            } else {
                                Some(self.sectors_with_claims.into_iter().map(|s| s.into_lotus_json()).collect())
                            },
                            new_expiration: self.new_expiration,
                        }
                    }

                    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                        Self {
                            deadline: lotus_json.deadline,
                            partition: lotus_json.partition,
                            sectors: lotus_json.sectors.clone(),
                            sectors_with_claims: if lotus_json.sectors_with_claims.is_none() {
                                vec![]
                            } else {
                                lotus_json.sectors_with_claims.unwrap().into_iter().map(|s| fil_actor_miner_state::[<v $version>]::SectorClaim::from_lotus_json(s)).collect()
                            },
                            new_expiration: lotus_json.new_expiration,
                        }
                    }
                }

                impl HasLotusJson for fil_actor_miner_state::[<v $version>]::SectorClaim {
                    type LotusJson = SectorClaimLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![(
                            json!({
                                "SectorNumber": 1,
                                "MaintainClaims": [1, 2],
                                "DropClaims": [3, 4]
                            }),
                            Self {
                                sector_number: 1,
                                maintain_claims: vec![1, 2],
                                drop_claims: vec![3, 4],
                            },
                        )]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        SectorClaimLotusJson {
                            sector_number: self.sector_number,
                            maintain_claims: self.maintain_claims,
                            drop_claims: self.drop_claims,
                        }
                    }

                    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                        Self {
                            sector_number: lotus_json.sector_number,
                            maintain_claims: lotus_json.maintain_claims,
                            drop_claims: lotus_json.drop_claims,
                        }
                    }
                }
            }
        )+
    };
}

// Add missing implementations for remaining parameter types
macro_rules! impl_lotus_json_for_miner_submit_windowed_post_params_v9_and_above {
    ($type_suffix:path: $($version:literal),+) => {
        $(
            paste! {
                mod [<impl_miner_submit_windowed_post_params_v9_and_above_ $version>] {
                    use super::*;
                    type T = fil_actor_miner_state::[<v $version>]::SubmitWindowedPoStParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }
                    impl HasLotusJson for T {
                        type LotusJson = SubmitWindowedPoStParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            let sectors = BitField::new();
                            vec![(
                                json!({
                                    "Deadline": 1,
                                    "Partitions": [
                                        {
                                            "Index": 0,
                                            "Skipped": [0]
                                        }
                                    ],
                                    "Proofs": [
                                        {
                                            "PoStProof": 1,
                                            "ProofBytes": "AQID"
                                        }
                                    ],
                                    "ChainCommitEpoch": 100,
                                    "ChainCommitRand": "AQID"
                                }),
                                Self {
                                    deadline: 1,
                                    partitions: vec![fil_actor_miner_state::[<v $version>]::PoStPartition{
                                        index: 0,
                                        skipped: sectors,
                                    }],
                                    proofs: vec![$type_suffix::sector::PoStProof{
                                        post_proof: crate::shim::sector::RegisteredPoStProof::from(1).into(),
                                        proof_bytes: vec![1, 2, 3],
                                    }],
                                    chain_commit_epoch: 100,
                                    chain_commit_rand: $type_suffix::randomness::Randomness(vec![1, 2, 3]),
                                },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            SubmitWindowedPoStParamsLotusJson {
                                deadline: self.deadline,
                                partitions: self.partitions.into_iter().map(|p| PoStPartitionLotusJson{
                                    index: p.index,
                                    skipped: p.skipped,
                                }).collect(),
                                proofs: self.proofs.into_iter().map(|p| PoStProof::new(
                                    p.post_proof.into(),
                                    p.proof_bytes,
                                )).collect(),
                                chain_commit_epoch: self.chain_commit_epoch,
                                chain_commit_rand: self.chain_commit_rand.0,
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                deadline: lotus_json.deadline,
                                partitions: lotus_json.partitions.into_iter().map(|p| fil_actor_miner_state::[<v $version>]::PoStPartition{
                                    index: p.index,
                                    skipped: p.skipped,
                                }).collect(),
                                proofs: lotus_json.proofs.into_iter().map(|p| $type_suffix::sector::PoStProof{
                                    post_proof: crate::shim::sector::RegisteredPoStProof::from(p.post_proof).into(),
                                    proof_bytes: p.proof_bytes.clone(),
                                }).collect(),
                                chain_commit_epoch: lotus_json.chain_commit_epoch,
                                chain_commit_rand: $type_suffix::randomness::Randomness(lotus_json.chain_commit_rand),
                            }
                        }
                    }
                }
            }
        )+
    };
}

impl HasLotusJson for fil_actor_miner_state::v8::SubmitWindowedPoStParams {
    type LotusJson = SubmitWindowedPoStParamsLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!({
                "Deadline": 1,
                "Partitions": [
                    {
                        "Index": 0,
                        "Skipped": [0]
                    }
                ],
                "Proofs": [
                    {
                        "PoStProof": 1,
                        "ProofBytes": "AQID"
                    }
                ],
                "ChainCommitEpoch": 100,
                "ChainCommitRand": "AQID"
            }),
            Self {
                deadline: 1,
                partitions: vec![fil_actor_miner_state::v8::PoStPartition {
                    index: 0,
                    skipped: BitField::new().into(),
                }],
                proofs: vec![fvm_shared2::sector::PoStProof {
                    post_proof: crate::shim::sector::RegisteredPoStProof::from(1).into(),
                    proof_bytes: vec![1, 2, 3],
                }],
                chain_commit_epoch: 100,
                chain_commit_rand: fvm_shared2::randomness::Randomness(vec![1, 2, 3]),
            },
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        SubmitWindowedPoStParamsLotusJson {
            deadline: self.deadline,
            partitions: self
                .partitions
                .into_iter()
                .map(|p| PoStPartitionLotusJson {
                    index: p.index,
                    skipped: p.skipped.try_into().unwrap_or_else(|_| BitField::new()),
                })
                .collect(),
            proofs: self
                .proofs
                .into_iter()
                .map(|p| PoStProof::new(p.post_proof.into(), p.proof_bytes))
                .collect(),
            chain_commit_epoch: self.chain_commit_epoch,
            chain_commit_rand: self.chain_commit_rand.0,
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        Self {
            deadline: lotus_json.deadline,
            partitions: lotus_json
                .partitions
                .into_iter()
                .map(|p| fil_actor_miner_state::v8::PoStPartition {
                    index: p.index,
                    skipped: p.skipped.into(),
                })
                .collect(),
            proofs: lotus_json
                .proofs
                .into_iter()
                .map(|p| fvm_shared2::sector::PoStProof {
                    post_proof: crate::shim::sector::RegisteredPoStProof::from(p.post_proof).into(),
                    proof_bytes: p.proof_bytes.clone(),
                })
                .collect(),
            chain_commit_epoch: lotus_json.chain_commit_epoch,
            chain_commit_rand: fvm_shared2::randomness::Randomness(lotus_json.chain_commit_rand),
        }
    }
}

macro_rules! impl_lotus_json_for_miner_post_partition_v9_and_above {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_miner_post_partition_v9_and_above_ $version>] {
                    use super::*;
                    type T = fil_actor_miner_state::[<v $version>]::PoStPartition;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }
                    impl HasLotusJson for T {
                        type LotusJson = PoStPartitionLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(
                                json!({
                                    "Index": 1,
                                    "Skipped": [0]
                                }),
                                Self {
                                    index: 1,
                                    skipped: BitField::new(),
                                },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            PoStPartitionLotusJson {
                                index: self.index,
                                skipped: self.skipped,
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                index: lotus_json.index,
                                skipped: lotus_json.skipped,
                            }
                        }
                    }
                }
            }
        )+
    };
}

impl HasLotusJson for fil_actor_miner_state::v8::PoStPartition {
    type LotusJson = PoStPartitionLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!({
                 "Index": 1,
                "Skipped": false
            }),
            Self {
                index: 1,
                skipped: BitField::new().into(),
            },
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        PoStPartitionLotusJson {
            index: self.index,
            skipped: self.skipped.try_into().unwrap_or_else(|_| BitField::new()),
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        Self {
            index: lotus_json.index,
            skipped: lotus_json.skipped.into(),
        }
    }
}

macro_rules! impl_lotus_json_for_miner_terminate_sectors_params_v9_and_above {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_miner_terminate_sectors_params_v9_and_above_ $version>] {
                    use super::*;
                    type T = fil_actor_miner_state::[<v $version>]::TerminateSectorsParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }
                    impl HasLotusJson for T {
                        type LotusJson = TerminateSectorsParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            let sectors = BitField::new();
                            vec![(
                                json!({
                                    "Terminations": [
                                        {
                                            "Deadline": 1,
                                            "Partition": 2,
                                            "Sectors": [0]
                                        }
                                    ]
                                }),
                                Self {
                                    terminations: vec![fil_actor_miner_state::[<v $version>]::TerminationDeclaration {
                                        deadline: 1,
                                        partition: 2,
                                        sectors: sectors.into(),
                                    }],
                                },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            TerminateSectorsParamsLotusJson {
                                terminations: self.terminations.into_iter().map(|t| t.into_lotus_json()).collect(),
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                terminations: lotus_json.terminations.into_iter().map(|t| fil_actor_miner_state::[<v $version>]::TerminationDeclaration::from_lotus_json(t)).collect(),
                            }
                        }
                    }
                }
            }
        )+
    };
}

macro_rules! impl_lotus_json_for_miner_termination_declaration_v9_and_above {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_miner_termination_declaration_v9_and_above_ $version>] {
                    use super::*;
                    type T = fil_actor_miner_state::[<v $version>]::TerminationDeclaration;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }
                    impl HasLotusJson for T {
                        type LotusJson = TerminationDeclarationLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(
                                json!({
                                    "Deadline": 1,
                                    "Partition": 2,
                                    "Sectors": [0]
                                }),
                                Self {
                                    deadline: 1,
                                    partition: 2,
                                    sectors: BitField::new(),
                                },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            TerminationDeclarationLotusJson {
                                deadline: self.deadline,
                                partition: self.partition,
                                sectors: self.sectors.try_into().unwrap_or_else(|_| BitField::new()),
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                deadline: lotus_json.deadline,
                                partition: lotus_json.partition,
                                sectors: lotus_json.sectors.into(),
                            }
                        }
                    }
                }
            }
        )+
    };
}

impl HasLotusJson for fil_actor_miner_state::v8::TerminationDeclaration {
    type LotusJson = TerminationDeclarationLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!({
                "Deadline": 1,
                "Partition": 2,
                "Sectors": [0]
            }),
            Self {
                deadline: 1,
                partition: 2,
                sectors: BitField::new().into(),
            },
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        TerminationDeclarationLotusJson {
            deadline: self.deadline,
            partition: self.partition,
            sectors: self.sectors.try_into().unwrap_or_else(|_| BitField::new()),
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        Self {
            deadline: lotus_json.deadline,
            partition: lotus_json.partition,
            sectors: lotus_json.sectors.into(),
        }
    }
}

macro_rules! impl_lotus_json_for_miner_declare_faults_params {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_miner_declare_faults_params_ $version>] {
                    use super::*;
                    type T = fil_actor_miner_state::[<v $version>]::DeclareFaultsParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }
                    impl HasLotusJson for T {
                        type LotusJson = DeclareFaultsParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(
                                json!({
                                    "Faults": [
                                        {
                                            "Deadline": 3,
                                            "Partition": 0,
                                            "Sectors": [0]
                                        }
                                    ]
                                }),
                                Self {
                                    faults: vec![fil_actor_miner_state::[<v $version>]::FaultDeclaration {
                                        deadline: 3,
                                        partition: 0,
                                        sectors: BitField::new().into(),
                                    }],
                                },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            DeclareFaultsParamsLotusJson {
                                faults: self.faults.into_iter().map(|f| f.into_lotus_json()).collect(),
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                faults: lotus_json.faults.into_iter().map(|f| fil_actor_miner_state::[<v $version>]::FaultDeclaration::from_lotus_json(f)).collect(),
                            }
                        }
                    }
                }
            }
        )+
    };
}

macro_rules! impl_lotus_json_for_miner_declare_faults_params_v9_and_above {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_miner_declare_faults_params_v9_and_above_ $version>] {
                    use super::*;
                    type T = fil_actor_miner_state::[<v $version>]::FaultDeclaration;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }
                    impl HasLotusJson for T {
                        type LotusJson = FaultDeclarationLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(
                                json!({
                                    "Deadline": 3,
                                    "Partition": 0,
                                    "Sectors": [0]
                                }),
                                Self {
                                    deadline: 3,
                                    partition: 0,
                                    sectors: BitField::new().into(),
                                },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            FaultDeclarationLotusJson {
                                deadline: self.deadline,
                                partition: self.partition,
                                sectors: self.sectors.try_into().unwrap_or_else(|_| BitField::new()),
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                deadline: lotus_json.deadline,
                                partition: lotus_json.partition,
                                sectors: lotus_json.sectors.into(),
                            }
                        }
                    }
                }
            }
        )+
    };
}

impl HasLotusJson for fil_actor_miner_state::v8::FaultDeclaration {
    type LotusJson = FaultDeclarationLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!({
                "Deadline": 3,
                "Partition": 0,
                "Sectors": [0]
            }),
            Self {
                deadline: 3,
                partition: 0,
                sectors: BitField::new().into(),
            },
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        FaultDeclarationLotusJson {
            deadline: self.deadline,
            partition: self.partition,
            sectors: self.sectors.try_into().unwrap_or_else(|_| BitField::new()),
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        Self {
            deadline: lotus_json.deadline,
            partition: lotus_json.partition,
            sectors: lotus_json.sectors.into(),
        }
    }
}

macro_rules! impl_lotus_json_for_miner_withdraw_balance_params {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_miner_withdraw_balance_params_ $version>] {
                    use super::*;
                    type T = fil_actor_miner_state::[<v $version>]::WithdrawBalanceParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }
                    impl HasLotusJson for T {
                        type LotusJson = WithdrawBalanceParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(
                                json!({"AmountRequested": "5000000000000000000"}),
                                Self { amount_requested: TokenAmount::from_atto(5_000_000_000_000_000_000_u64).into() },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            WithdrawBalanceParamsLotusJson {
                                amount_requested: self.amount_requested.into(),
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                amount_requested: lotus_json.amount_requested.into(),
                            }
                        }
                    }
                }
            }
        )+
    };
}

macro_rules! impl_lotus_json_for_miner_change_multiaddrs_params {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_miner_change_multiaddrs_params_ $version>] {
                    use super::*;
                    type T = fil_actor_miner_state::[<v $version>]::ChangeMultiaddrsParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }
                    impl HasLotusJson for T {
                        type LotusJson = ChangeMultiaddrsParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(
                                json!({"NewMultiaddrs": ["L2lwNC8xMjcuMC4wLjEvdGNwLzgwODA=", "L2Rucy9leGFtcGxlLmNvbQ=="]}),
                                Self {
                                    new_multi_addrs: vec![
                                        BytesDe(b"/ip4/127.0.0.1/tcp/8080".to_vec()),
                                        BytesDe(b"/dns/example.com".to_vec()),
                                    ],
                                },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            ChangeMultiaddrsParamsLotusJson {
                                new_multi_addrs: self.new_multi_addrs.into_iter().map(|addr| addr.0).collect(),
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                new_multi_addrs: lotus_json.new_multi_addrs.into_iter().map(BytesDe).collect(),
                            }
                        }
                    }
                }
            }
        )+
    };
}

macro_rules! impl_lotus_json_for_miner_compact_partitions_params {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_miner_compact_partitions_params_ $version>] {
                    use super::*;
                    type T = fil_actor_miner_state::[<v $version>]::CompactPartitionsParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }
                    impl HasLotusJson for T {
                        type LotusJson = CompactPartitionsParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(
                                json!({
                                    "Deadline": 1,
                                    "Partitions": [0]
                                }),
                                Self {
                                    deadline: 1,
                                    partitions: BitField::new(),
                                },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            CompactPartitionsParamsLotusJson {
                                deadline: self.deadline,
                                partitions: self.partitions,
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                deadline: lotus_json.deadline,
                                partitions: lotus_json.partitions,
                            }
                        }
                    }
                }
            }
        )+
    };
}

impl HasLotusJson for fil_actor_miner_state::v8::CompactPartitionsParams {
    type LotusJson = CompactPartitionsParamsLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!({
                "Deadline": 1,
                "Partitions": [0]
            }),
            Self {
                deadline: 1,
                partitions: BitField::new().into(),
            },
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        CompactPartitionsParamsLotusJson {
            deadline: self.deadline,
            partitions: self
                .partitions
                .try_into()
                .unwrap_or_else(|_| BitField::new()),
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        Self {
            deadline: lotus_json.deadline,
            partitions: lotus_json.partitions.into(),
        }
    }
}

macro_rules! impl_lotus_json_for_miner_compact_sector_numbers_params {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_miner_compact_sector_numbers_params_ $version>] {
                    use super::*;
                    type T = fil_actor_miner_state::[<v $version>]::CompactSectorNumbersParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }
                    impl HasLotusJson for T {
                        type LotusJson = CompactSectorNumbersParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(
                                json!({
                                    "MaskSectorNumbers": [0]
                                }),
                                Self {
                                    mask_sector_numbers: BitField::new(),
                                },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            CompactSectorNumbersParamsLotusJson {
                                mask_sector_numbers: self.mask_sector_numbers,
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                mask_sector_numbers: lotus_json.mask_sector_numbers,
                            }
                        }
                    }
                }
            }
        )+
    };
}

impl HasLotusJson for fil_actor_miner_state::v8::CompactSectorNumbersParams {
    type LotusJson = CompactSectorNumbersParamsLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!({
                "MaskSectorNumbers": [0]
            }),
            Self {
                mask_sector_numbers: BitField::new().into(),
            },
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        CompactSectorNumbersParamsLotusJson {
            mask_sector_numbers: self
                .mask_sector_numbers
                .try_into()
                .unwrap_or_else(|_| BitField::new()),
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        Self {
            mask_sector_numbers: lotus_json.mask_sector_numbers.into(),
        }
    }
}

macro_rules! impl_lotus_json_for_miner_dispute_windowed_post_params {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_miner_dispute_windowed_post_params_ $version>] {
                    use super::*;
                    type T = fil_actor_miner_state::[<v $version>]::DisputeWindowedPoStParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }
                    impl HasLotusJson for T {
                        type LotusJson = DisputeWindowedPoStParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(
                                json!({
                                    "Deadline": 1,
                                    "PoStIndex": 2
                                }),
                                Self {
                                    deadline: 1,
                                    post_index: 2,
                                },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            DisputeWindowedPoStParamsLotusJson {
                                deadline: self.deadline,
                                post_index: self.post_index,
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                deadline: lotus_json.deadline,
                                post_index: lotus_json.post_index,
                            }
                        }
                    }
                }
            }
        )+
    };
}

macro_rules! impl_lotus_json_for_miner_pre_commit_sector_batch2_params {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_miner_pre_commit_sector_batch2_params_ $version>] {
                    use super::*;
                    type T = fil_actor_miner_state::[<v $version>]::PreCommitSectorBatchParams2;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }
                    impl HasLotusJson for T {
                        type LotusJson = PreCommitSectorBatch2ParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            let test_cid = Cid::default();
                            vec![(
                                json!({
                                    "Sectors": [
                                        {
                                            "SealProof": 1,
                                            "SectorNumber": 100,
                                            "SealedCID": {"/": "baeaaaaa"},
                                            "SealRandEpoch": 1000,
                                            "DealIDs": [1, 2, 3],
                                            "Expiration": 2000,
                                            "UnsealedCid": null
                                        }
                                    ]
                                }),
                                Self {
                                    sectors: vec![fil_actor_miner_state::[<v $version>]::SectorPreCommitInfo {
                                        seal_proof: crate::shim::sector::RegisteredSealProof::from(1).into(),
                                        sector_number: 100,
                                        sealed_cid: test_cid,
                                        seal_rand_epoch: 1000,
                                        deal_ids: vec![1, 2, 3],
                                        expiration: 2000,
                                        unsealed_cid: fil_actor_miner_state::[<v $version>]::CompactCommD(None),
                                    }],
                                },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            PreCommitSectorBatch2ParamsLotusJson {
                                sectors: self.sectors.into_iter().map(|s| s.into_lotus_json()).collect(),
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                sectors: lotus_json.sectors.into_iter().map(|s| fil_actor_miner_state::[<v $version>]::SectorPreCommitInfo::from_lotus_json(s)).collect(),
                            }
                        }
                    }
                }

                impl HasLotusJson for fil_actor_miner_state::[<v $version>]::SectorPreCommitInfo {
                    type LotusJson = SectorPreCommitInfoLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![(
                            json!({
                                "SealProof": 1,
                                "SectorNumber": 100,
                                "SealedCID": {"/": "baeaaaaa"},
                                "SealRandEpoch": 1000,
                                "DealIDs": [1, 2, 3],
                                "Expiration": 2000,
                                "UnsealedCid": null
                            }),
                            Self {
                                seal_proof: crate::shim::sector::RegisteredSealProof::from(1).into(),
                                sector_number: 100,
                                sealed_cid: Cid::default(),
                                seal_rand_epoch: 1000,
                                deal_ids: vec![1, 2, 3],
                                expiration: 2000,
                                unsealed_cid: fil_actor_miner_state::[<v $version>]::CompactCommD(None),
                            },
                        )]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        SectorPreCommitInfoLotusJson {
                            seal_proof: self.seal_proof.into(),
                            sector_number: self.sector_number,
                            sealed_cid: self.sealed_cid,
                            seal_rand_epoch: self.seal_rand_epoch,
                            deal_ids: if self.deal_ids.is_empty() { None } else { Some(self.deal_ids.clone())},
                            expiration: self.expiration,
                            unsealed_cid: self.unsealed_cid.0,
                        }
                    }

                    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                        Self {
                            seal_proof: crate::shim::sector::RegisteredSealProof::from(lotus_json.seal_proof).into(),
                            sector_number: lotus_json.sector_number,
                            sealed_cid: lotus_json.sealed_cid,
                            seal_rand_epoch: lotus_json.seal_rand_epoch,
                            deal_ids: if lotus_json.deal_ids.is_none() { vec![] } else { lotus_json.deal_ids.unwrap() },
                            expiration: lotus_json.expiration,
                            unsealed_cid: fil_actor_miner_state::[<v $version>]::CompactCommD(lotus_json.unsealed_cid),
                        }
                    }
                }
            }
        )+
    };
}

macro_rules! impl_lotus_json_for_miner_pre_commit_sector_params {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_miner_pre_commit_sector_params_ $version>] {
                    use super::*;
                    type T = fil_actor_miner_state::[<v $version>]::PreCommitSectorParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }
                    impl HasLotusJson for T {
                        type LotusJson = PreCommitSectorParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(
                                json!({
                                    "SealProof": 1,
                                    "SectorNumber": 100,
                                    "SealedCID": {"/": "baeaaaaa"},
                                    "SealRandEpoch": 1000,
                                    "DealIDs": [1, 2, 3],
                                    "Expiration": 2000,
                                    "ReplaceCapacity": false,
                                    "ReplaceSectorDeadline": 0,
                                    "ReplaceSectorPartition": 0,
                                    "ReplaceSectorNumber": 0
                                }),
                                Self {
                                    seal_proof: crate::shim::sector::RegisteredSealProof::from(1).into(),
                                    sector_number: 100,
                                    sealed_cid: Cid::default(),
                                    seal_rand_epoch: 1000,
                                    deal_ids: vec![1, 2, 3],
                                    expiration: 2000,
                                    replace_capacity: false,
                                    replace_sector_deadline: 0,
                                    replace_sector_partition: 0,
                                    replace_sector_number: 0,
                                },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            PreCommitSectorParamsLotusJson {
                                seal_proof: self.seal_proof.into(),
                                sector_number: self.sector_number,
                                sealed_cid: self.sealed_cid,
                                seal_rand_epoch: self.seal_rand_epoch,
                                deal_ids: self.deal_ids,
                                expiration: self.expiration,
                                replace_capacity: self.replace_capacity,
                                replace_sector_deadline: self.replace_sector_deadline,
                                replace_sector_partition: self.replace_sector_partition,
                                replace_sector_number: self.replace_sector_number,
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                seal_proof: lotus_json.seal_proof.into(),
                                sector_number: lotus_json.sector_number,
                                sealed_cid: lotus_json.sealed_cid,
                                seal_rand_epoch: lotus_json.seal_rand_epoch,
                                deal_ids: lotus_json.deal_ids,
                                expiration: lotus_json.expiration,
                                replace_capacity: lotus_json.replace_capacity,
                                replace_sector_deadline: lotus_json.replace_sector_deadline,
                                replace_sector_partition: lotus_json.replace_sector_partition,
                                replace_sector_number: lotus_json.replace_sector_number,
                            }
                        }
                    }
                }
            }
        )+
    };
}

impl HasLotusJson for fil_actor_miner_state::v8::PreCommitSectorBatchParams {
    type LotusJson = PreCommitSectorBatchParamsLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!({
                "Sectors": [
                    {
                        "SealProof": 1,
                        "SectorNumber": 100,
                        "SealedCID": {"/": "baeaaaaa"},
                        "SealRandEpoch": 1000,
                        "DealIDs": [1, 2, 3],
                        "Expiration": 2000,
                        "ReplaceCapacity": false,
                        "ReplaceSectorDeadline": 0,
                        "ReplaceSectorPartition": 0,
                        "ReplaceSectorNumber": 0
                    }
                ]
            }),
            Self {
                sectors: vec![fil_actor_miner_state::v8::SectorPreCommitInfo {
                    seal_proof: crate::shim::sector::RegisteredSealProof::from(1).into(),
                    sector_number: 100,
                    sealed_cid: Cid::default(),
                    seal_rand_epoch: 1000,
                    deal_ids: vec![1, 2, 3],
                    expiration: 2000,
                    replace_capacity: false,
                    replace_sector_deadline: 0,
                    replace_sector_partition: 0,
                    replace_sector_number: 0,
                }],
            },
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        PreCommitSectorBatchParamsLotusJson {
            sectors: self
                .sectors
                .into_iter()
                .map(|s| PreCommitSectorParamsLotusJson {
                    seal_proof: s.seal_proof.into(),
                    sector_number: s.sector_number,
                    sealed_cid: s.sealed_cid,
                    seal_rand_epoch: s.seal_rand_epoch,
                    deal_ids: s.deal_ids,
                    expiration: s.expiration,
                    replace_capacity: s.replace_capacity,
                    replace_sector_deadline: s.replace_sector_deadline,
                    replace_sector_partition: s.replace_sector_partition,
                    replace_sector_number: s.replace_sector_number,
                })
                .collect(),
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        Self {
            sectors: lotus_json
                .sectors
                .into_iter()
                .map(|s| fil_actor_miner_state::v8::SectorPreCommitInfo {
                    seal_proof: s.seal_proof.into(),
                    sector_number: s.sector_number,
                    sealed_cid: s.sealed_cid,
                    seal_rand_epoch: s.seal_rand_epoch,
                    deal_ids: s.deal_ids,
                    expiration: s.expiration,
                    replace_capacity: s.replace_capacity,
                    replace_sector_deadline: s.replace_sector_deadline,
                    replace_sector_partition: s.replace_sector_partition,
                    replace_sector_number: s.replace_sector_number,
                })
                .collect(),
        }
    }
}

macro_rules! impl_lotus_json_for_miner_pre_commit_sector_and_batch_params {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_miner_pre_commit_sector_and_batch_params_ $version>] {
                    use super::*;
                    type T = fil_actor_miner_state::[<v $version>]::PreCommitSectorBatchParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }
                    impl HasLotusJson for T {
                        type LotusJson = PreCommitSectorBatchParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(
                                json!({
                                    "Sectors": [
                                        {
                                            "SealProof": 1,
                                            "SectorNumber": 100,
                                            "SealedCID": {"/": "baeaaaaa"},
                                            "SealRandEpoch": 1000,
                                            "DealIDs": [1, 2, 3],
                                            "Expiration": 2000,
                                            "ReplaceCapacity": false,
                                            "ReplaceSectorDeadline": 0,
                                            "ReplaceSectorPartition": 0,
                                            "ReplaceSectorNumber": 0
                                        }
                                    ]
                                }),
                                Self {
                                    sectors: vec![fil_actor_miner_state::[<v $version>]::PreCommitSectorParams {
                                        seal_proof: crate::shim::sector::RegisteredSealProof::from(1).into(),
                                        sector_number: 100,
                                        sealed_cid: Cid::default(),
                                        seal_rand_epoch: 1000,
                                        deal_ids: vec![1, 2, 3],
                                        expiration: 2000,
                                        replace_capacity: false,
                                        replace_sector_deadline: 0,
                                        replace_sector_partition: 0,
                                        replace_sector_number: 0,
                                    }],
                                },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            PreCommitSectorBatchParamsLotusJson {
                                sectors: self.sectors.into_iter().map(|s| PreCommitSectorParamsLotusJson {
                                    seal_proof: s.seal_proof.into(),
                                    sector_number: s.sector_number,
                                    sealed_cid: s.sealed_cid,
                                    seal_rand_epoch: s.seal_rand_epoch,
                                    deal_ids: s.deal_ids,
                                    expiration: s.expiration,
                                    replace_capacity: s.replace_capacity,
                                    replace_sector_deadline: s.replace_sector_deadline,
                                    replace_sector_partition: s.replace_sector_partition,
                                    replace_sector_number: s.replace_sector_number,
                                }).collect(),
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                sectors: lotus_json.sectors.into_iter().map(|s| fil_actor_miner_state::[<v $version>]::PreCommitSectorParams {
                                    seal_proof: s.seal_proof.into(),
                                    sector_number: s.sector_number,
                                    sealed_cid: s.sealed_cid,
                                    seal_rand_epoch: s.seal_rand_epoch,
                                    deal_ids: s.deal_ids,
                                    expiration: s.expiration,
                                    replace_capacity: s.replace_capacity,
                                    replace_sector_deadline: s.replace_sector_deadline,
                                    replace_sector_partition: s.replace_sector_partition,
                                    replace_sector_number: s.replace_sector_number,
                                }).collect(),
                            }
                        }
                    }
                }
            }
        )+
    };
}

macro_rules! impl_lotus_json_for_miner_prove_commit_sectors3_params {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_miner_prove_commit_sectors3_params_ $version>] {
                    use super::*;
                    type T = fil_actor_miner_state::[<v $version>]::ProveCommitSectors3Params;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }
                    impl HasLotusJson for T {
                        type LotusJson = ProveCommitSectors3ParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(
                                json!({
                                    "SectorActivations": [
                                        {
                                            "SectorNumber": 100,
                                            "Pieces": [
                                                {
                                                    "CID": {"/": "baeaaaaa"},
                                                    "Size": 2048,
                                                    "VerifiedAllocationKey": null,
                                                    "Notify": null
                                                }
                                            ]
                                        }
                                    ],
                                    "SectorProofs": ["AQID"],
                                    "AggregateProof": "AQID",
                                    "AggregateProofType": 1,
                                    "RequireActivationSuccess": true,
                                    "RequireNotificationSuccess": false
                                }),
                                Self {
                                    sector_activations: vec![fil_actor_miner_state::[<v $version>]::SectorActivationManifest {
                                        sector_number: 100,
                                        pieces: vec![fil_actor_miner_state::[<v $version>]::PieceActivationManifest {
                                            cid: Cid::default(),
                                            size: fvm_shared4::piece::PaddedPieceSize(2048),
                                            verified_allocation_key: None,
                                            notify: vec![],
                                        }],
                                    }],
                                    sector_proofs: vec![RawBytes::new(vec![1, 2, 3])],
                                    aggregate_proof: RawBytes::new(vec![1, 2, 3]),
                                    aggregate_proof_type: Some(fvm_shared4::sector::RegisteredAggregateProof::from(1)),
                                    require_activation_success: true,
                                    require_notification_success: false,
                                },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            ProveCommitSectors3ParamsLotusJson {
                                sector_activations: self.sector_activations.into_iter().map(|s| SectorActivationManifestLotusJson{
                                    sector_number: s.sector_number,
                                    pieces: s.pieces.into_iter().map(|p| PieceActivationManifestLotusJson{
                                        cid: p.cid,
                                        size: p.size.0,
                                        verified_allocation_key: p.verified_allocation_key.map(|v| VerifiedAllocationKeyLotusJson{
                                            id: v.id,
                                            client: v.client,
                                        }),
                                        notify: (!p.notify.is_empty()).then_some(
                                            p.notify
                                                .into_iter()
                                                .map(|n| n.into_lotus_json())
                                                .collect_vec()
                                        ),
                                    }).collect(),
                                }).collect(),
                                sector_proofs: self.sector_proofs,
                                aggregate_proof: self.aggregate_proof,
                                aggregate_proof_type: self.aggregate_proof_type.map(|t| i64::from(t)),
                                require_activation_success: self.require_activation_success,
                                require_notification_success: self.require_notification_success,
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                sector_activations: lotus_json.sector_activations.into_iter().map(|s| fil_actor_miner_state::[<v $version>]::SectorActivationManifest{
                                    sector_number: s.sector_number,
                                    pieces: s.pieces.into_iter().map(|p| fil_actor_miner_state::[<v $version>]::PieceActivationManifest{
                                        cid: p.cid,
                                        size: fvm_shared4::piece::PaddedPieceSize(p.size),
                                        verified_allocation_key: p.verified_allocation_key.map(|v| fil_actor_miner_state::[<v $version>]::VerifiedAllocationKey{
                                            id: v.id,
                                            client: v.client,
                                        }),
                                        notify: p.notify
                                            .unwrap_or_default()
                                            .into_iter()
                                            .map(|n| fil_actor_miner_state::[<v $version>]::DataActivationNotification::from_lotus_json(n))
                                            .collect(),
                                    }).collect(),
                                }).collect(),
                                sector_proofs: lotus_json.sector_proofs,
                                aggregate_proof: lotus_json.aggregate_proof,
                                aggregate_proof_type: lotus_json.aggregate_proof_type.map(|t| fvm_shared4::sector::RegisteredAggregateProof::from(t)),
                                require_activation_success: lotus_json.require_activation_success,
                                require_notification_success: lotus_json.require_notification_success,
                            }
                        }
                    }
                }
            }
        )+
    };
}

macro_rules! impl_lotus_json_for_miner_prove_replica_updates3_params {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_miner_prove_replica_updates3_params_ $version>] {
                    use super::*;
                    type T = fil_actor_miner_state::[<v $version>]::ProveReplicaUpdates3Params;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }
                    impl HasLotusJson for T {
                        type LotusJson = ProveReplicaUpdates3ParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(
                                json!({
                                    "SectorUpdates": [
                                        {
                                            "Sector": 100,
                                            "Deadline": 1,
                                            "Partition": 2,
                                            "NewSealedCID": {"/": "baeaaaaa"},
                                            "Pieces": [
                                                {
                                                    "CID": {"/": "baeaaaaa"},
                                                    "Size": 2048,
                                                    "VerifiedAllocationKey": null,
                                                    "Notify": null
                                                }
                                            ]
                                        }
                                    ],
                                    "SectorProofs": ["AQID"],
                                    "AggregateProof": "AQID",
                                    "UpdateProofsType": 1,
                                    "AggregateProofType": 1,
                                    "RequireActivationSuccess": true,
                                    "RequireNotificationSuccess": false
                                }),
                                Self {
                                    sector_updates: vec![fil_actor_miner_state::[<v $version>]::SectorUpdateManifest {
                                        sector: 100,
                                        deadline: 1,
                                        partition: 2,
                                        new_sealed_cid: Cid::default(),
                                        pieces: vec![fil_actor_miner_state::[<v $version>]::PieceActivationManifest {
                                            cid: Cid::default(),
                                            size: fvm_shared4::piece::PaddedPieceSize(2048),
                                            verified_allocation_key: None,
                                            notify: vec![],
                                        }],
                                    }],
                                    sector_proofs: vec![RawBytes::new(vec![1, 2, 3])],
                                    update_proofs_type: RegisteredUpdateProof::from(1),
                                    aggregate_proof: RawBytes::new(vec![1, 2, 3]),
                                    aggregate_proof_type: Some(fvm_shared4::sector::RegisteredAggregateProof::from(1)),
                                    require_activation_success: true,
                                    require_notification_success: false,
                                },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            ProveReplicaUpdates3ParamsLotusJson {
                                sector_updates: self.sector_updates.into_iter().map(|s| SectorUpdateManifestLotusJson{
                                    sector: s.sector,
                                    deadline: s.deadline,
                                    partition: s.partition,
                                    new_sealed_cid: s.new_sealed_cid,
                                    pieces: s.pieces.into_iter().map(|p| PieceActivationManifestLotusJson {
                                        cid: p.cid,
                                        size: p.size.0,
                                        verified_allocation_key: p.verified_allocation_key.map(|v| VerifiedAllocationKeyLotusJson{
                                            id: v.id.into(),
                                            client: v.client.into(),
                                        }),
                                        notify: (!p.notify.is_empty()).then_some(
                                            p.notify
                                                .into_iter()
                                                .map(|n| n.into_lotus_json())
                                                .collect_vec()
                                        ),
                                    }).collect(),
                                }).collect(),
                                sector_proofs: self.sector_proofs,
                                aggregate_proof: self.aggregate_proof,
                                update_proofs_type: i64::from(self.update_proofs_type),
                                aggregate_proof_type: self.aggregate_proof_type.map(|t| i64::from(t)),
                                require_activation_success: self.require_activation_success,
                                require_notification_success: self.require_notification_success,
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                sector_updates: lotus_json.sector_updates.into_iter().map(|s| fil_actor_miner_state::[<v $version>]::SectorUpdateManifest{
                                    sector: s.sector,
                                    deadline: s.deadline,
                                    partition: s.partition,
                                    new_sealed_cid: s.new_sealed_cid,
                                    pieces: s.pieces.into_iter().map(|p| fil_actor_miner_state::[<v $version>]::PieceActivationManifest{
                                        cid: p.cid,
                                        size: fvm_shared4::piece::PaddedPieceSize(p.size),
                                        verified_allocation_key: p.verified_allocation_key.map(|v| fil_actor_miner_state::[<v $version>]::VerifiedAllocationKey{
                                            id: v.id,
                                            client: v.client,
                                        }),
                                        notify: p.notify
                                            .unwrap_or_default()
                                            .into_iter()
                                            .map(|n| fil_actor_miner_state::[<v $version>]::DataActivationNotification::from_lotus_json(n))
                                            .collect(),
                                    }).collect(),
                                }).collect(),
                                sector_proofs: lotus_json.sector_proofs,
                                update_proofs_type: RegisteredUpdateProof::from(lotus_json.update_proofs_type),
                                aggregate_proof: lotus_json.aggregate_proof,
                                aggregate_proof_type: lotus_json.aggregate_proof_type.map(|t| fvm_shared4::sector::RegisteredAggregateProof::from(t)),
                                require_activation_success: lotus_json.require_activation_success,
                                require_notification_success: lotus_json.require_notification_success,
                            }
                        }
                    }
                }
            }
        )+
    };
}

macro_rules! impl_lotus_json_for_miner_report_consensus_fault_params {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_miner_report_consensus_fault_params_ $version>] {
                    use super::*;
                    type T = fil_actor_miner_state::[<v $version>]::ReportConsensusFaultParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }
                    impl HasLotusJson for T {
                        type LotusJson = ReportConsensusFaultParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(
                                json!({
                                    "BlockHeader1": "AQID",
                                    "BlockHeader2": "BAUG",
                                    "BlockHeaderExtra": "Cg=="
                                }),
                                Self {
                                    header1: vec![1, 2, 3],
                                    header2: vec![4, 5, 6],
                                    header_extra: vec![10],
                                },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            ReportConsensusFaultParamsLotusJson {
                                header1: self.header1,
                                header2: self.header2,
                                header_extra: self.header_extra,
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                header1: lotus_json.header1,
                                header2: lotus_json.header2,
                                header_extra: lotus_json.header_extra,
                            }
                        }
                    }
                }
            }
        )+
    };
}

macro_rules! impl_lotus_json_for_miner_check_sector_proven_params {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_miner_check_sector_proven_params_ $version>] {
                    use super::*;
                    type T = fil_actor_miner_state::[<v $version>]::CheckSectorProvenParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }
                    impl HasLotusJson for T {
                        type LotusJson = CheckSectorProvenParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(
                                json!({
                                    "SectorNumber": 100
                                }),
                                Self {
                                    sector_number: 100,
                                },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            CheckSectorProvenParamsLotusJson {
                                sector_number: self.sector_number,
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                sector_number: lotus_json.sector_number,
                            }
                        }
                    }
                }
            }
        )+
    };
}

macro_rules! impl_lotus_json_for_miner_apply_reward_params {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_miner_apply_reward_params_ $version>] {
                    use super::*;
                    type T = fil_actor_miner_state::[<v $version>]::ApplyRewardParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }
                    impl HasLotusJson for T {
                        type LotusJson = ApplyRewardParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(
                                json!({
                                    "Reward": "1000000000000000000",
                                    "Penalty": "500000000000000000"
                                }),
                                Self {
                                    reward: TokenAmount::from_atto(1000000000000000000u64).into(),
                                    penalty: TokenAmount::from_atto(500000000000000000u64).into(),
                                },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            ApplyRewardParamsLotusJson {
                                reward: self.reward.into(),
                                penalty: self.penalty.into(),
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                reward: lotus_json.reward.into(),
                                penalty: lotus_json.penalty.into(),
                            }
                        }
                    }
                }
            }
        )+
    };
}

macro_rules! impl_lotus_json_for_miner_prove_commit_aggregate_params_v9_to_v16 {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_miner_prove_commit_aggregate_params_v9_to_v16_ $version>] {
                    use super::*;
                    type T = fil_actor_miner_state::[<v $version>]::ProveCommitAggregateParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }
                    impl HasLotusJson for T {
                        type LotusJson = ProveCommitAggregateParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(
                                json!({
                                    "SectorNumbers": [0],
                                    "AggregateProof": "AQID"
                                }),
                                Self {
                                    sector_numbers: BitField::new(),
                                    aggregate_proof: vec![1, 2, 3].into(),
                                },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            ProveCommitAggregateParamsLotusJson {
                                sector_numbers: self.sector_numbers,
                                aggregate_proof: self.aggregate_proof.into(),
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                sector_numbers: lotus_json.sector_numbers,
                                aggregate_proof: lotus_json.aggregate_proof.into(),
                            }
                        }
                    }
                }
            }
        )+
    };
}

impl HasLotusJson for fil_actor_miner_state::v8::ProveCommitAggregateParams {
    type LotusJson = ProveCommitAggregateParamsLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!({
                "SectorNumbers": [0],
                "AggregateProof": "AQID"
            }),
            Self {
                sector_numbers: BitField::new().into(),
                aggregate_proof: vec![1, 2, 3],
            },
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        ProveCommitAggregateParamsLotusJson {
            sector_numbers: self
                .sector_numbers
                .try_into()
                .unwrap_or_else(|_| BitField::new()),
            aggregate_proof: self.aggregate_proof.into(),
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        Self {
            sector_numbers: lotus_json.sector_numbers.into(),
            aggregate_proof: lotus_json.aggregate_proof.into(),
        }
    }
}

macro_rules! impl_lotus_json_for_miner_prove_replica_updates_params {
    ($type_suffix:path: $($version:literal),+) => {
        $(
            paste! {
                mod [<impl_miner_prove_replica_updates_params_ $version>] {
                    use super::*;
                    type T = fil_actor_miner_state::[<v $version>]::ProveReplicaUpdatesParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }
                    impl HasLotusJson for T {
                        type LotusJson = ProveReplicaUpdatesParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(
                                json!({
                                    "Updates": [
                                        {
                                            "SectorID": 100,
                                            "Deadline": 1,
                                            "Partition": 2,
                                            "NewSealedSectorCID": {"/": "baeaaaaa"},
                                            "Deals": [1, 2, 3],
                                            "UpdateProofType": 1,
                                            "ReplicaProof": "AQID"
                                        }
                                    ]
                                }),
                                Self {
                                    updates: vec![fil_actor_miner_state::[<v $version>]::ReplicaUpdate {
                                        sector_number: 100,
                                        deadline: 1,
                                        partition: 2,
                                        new_sealed_cid: Cid::default(),
                                        deals: vec![1, 2, 3],
                                        update_proof_type: $type_suffix::sector::RegisteredUpdateProof::from(1),
                                        replica_proof: vec![1, 2, 3].into(),
                                    }],
                                },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            ProveReplicaUpdatesParamsLotusJson {
                                updates: self.updates.into_iter().map(|u| ReplicaUpdateLotusJson {
                                    sector_number: u.sector_number,
                                    deadline: u.deadline,
                                    partition: u.partition,
                                    new_sealed_cid: u.new_sealed_cid,
                                    deals: if u.deals.is_empty() { None } else { Some(u.deals) },
                                    update_proof_type: i64::from(u.update_proof_type),
                                    replica_proof: u.replica_proof.into(),
                                }).collect(),
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                updates: lotus_json.updates.into_iter().map(|u| fil_actor_miner_state::[<v $version>]::ReplicaUpdate{
                                    sector_number: u.sector_number,
                                    deadline: u.deadline,
                                    partition: u.partition,
                                    new_sealed_cid: u.new_sealed_cid,
                                    deals: u.deals.unwrap_or_default(),
                                    update_proof_type: u.update_proof_type.into(),
                                    replica_proof: u.replica_proof.into(),
                                }).collect(),
                            }
                        }
                    }
                }

                impl HasLotusJson for fil_actor_miner_state::[<v $version>]::ReplicaUpdate {
                    type LotusJson = ReplicaUpdateLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![(
                            json!({
                                "SectorNumber": 100,
                                "Deadline": 1,
                                "Partition": 2,
                                "NewSealedCid": {"/": "baeaaaaa"},
                                "Deals": [1, 2, 3],
                                "UpdateProofType": 1,
                                "ReplicaProof": "AQID"
                            }),
                            Self {
                                sector_number: 100,
                                deadline: 1,
                                partition: 2,
                                new_sealed_cid: Cid::default(),
                                deals: vec![1, 2, 3],
                                update_proof_type: $type_suffix::sector::RegisteredUpdateProof::from(1),
                                replica_proof: vec![1, 2, 3].into(),
                            },
                        )]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        ReplicaUpdateLotusJson {
                            sector_number: self.sector_number,
                            deadline: self.deadline,
                            partition: self.partition,
                            new_sealed_cid: self.new_sealed_cid,
                            deals: if self.deals.is_empty() { None } else { Some(self.deals) },
                            update_proof_type: i64::from(self.update_proof_type),
                            replica_proof: self.replica_proof.into(),
                        }
                    }

                    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                        Self {
                            sector_number: lotus_json.sector_number,
                            deadline: lotus_json.deadline,
                            partition: lotus_json.partition,
                            new_sealed_cid: lotus_json.new_sealed_cid,
                            deals: lotus_json.deals.unwrap_or_default(),
                            update_proof_type: $type_suffix::sector::RegisteredUpdateProof::from(lotus_json.update_proof_type),
                            replica_proof: lotus_json.replica_proof.into(),
                        }
                    }
                }
            }
        )+
    };
}

macro_rules! impl_lotus_json_for_miner_is_controlling_address_param {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_miner_is_controlling_address_param_ $version>] {
                    use super::*;
                    type T = fil_actor_miner_state::[<v $version>]::IsControllingAddressParam;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }
                    impl HasLotusJson for T {
                        type LotusJson = IsControllingAddressParamLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(
                                json!("f01234"),
                                Self {
                                    address: Address::new_id(1234).into(),
                                },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            IsControllingAddressParamLotusJson(self.address.into())
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                address: lotus_json.0.into(),
                            }
                        }
                    }
                }
            }
        )+
    };
}

macro_rules! impl_lotus_json_for_miner_max_termination_fee_params {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_miner_max_termination_fee_params_ $version>] {
                    use super::*;
                    type T = fil_actor_miner_state::[<v $version>]::MaxTerminationFeeParams;
                #[test]
                fn snapshots() {
                    crate::lotus_json::assert_all_snapshots::<T>();
                }
                impl HasLotusJson for T {
                    type LotusJson = MaxTerminationFeeParamsLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                    vec![(
                        json!({
                            "Power": "1000000000000000000",
                            "InitialPledge": "500000000000000000"
                        }),
                        Self {
                            power: BigInt::from(1000000000000000000u64),
                            initial_pledge: TokenAmount::from_atto(500000000000000000u64).into(),
                        },
                    )]
                }

                fn into_lotus_json(self) -> Self::LotusJson {
                    MaxTerminationFeeParamsLotusJson {
                        power: self.power,
                        initial_pledge: self.initial_pledge.into(),
                    }
                }

                fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                    Self {
                        power: lotus_json.power,
                        initial_pledge: lotus_json.initial_pledge.into(),
                    }
                    }
                }
            }
        }
        )+
    };
}

macro_rules! impl_lotus_json_for_miner_change_peer_id_params {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_miner_change_peer_id_params_ $version>] {
                    use super::*;
                    type T = fil_actor_miner_state::[<v $version>]::ChangePeerIDParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }
                    impl HasLotusJson for T {
                        type LotusJson = ChangePeerIDParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(
                                json!({"NewID": "AQID"}), // base64 for [1, 2, 3]
                                Self { new_id: vec![1, 2, 3] },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            ChangePeerIDParamsLotusJson {
                                new_id: self.new_id,
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                new_id: lotus_json.new_id,
                            }
                        }
                    }
                }
            }
        )+
    };
}

macro_rules! impl_lotus_json_for_miner_sector_activation_manifest {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_miner_sector_activation_manifest_ $version>] {
                    use super::*;
                    type T = fil_actor_miner_state::[<v $version>]::SectorActivationManifest;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }
                    impl HasLotusJson for T {
                        type LotusJson = SectorActivationManifestLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(
                                json!({
                                    "SectorNumber": 100,
                                    "Pieces": [
                                        {
                                            "CID": {"/": "baeaaaaa"},
                                            "Size": 2048,
                                            "VerifiedAllocationKey": null,
                                            "Notify": null
                                        }
                                    ]
                                }),
                                Self {
                                    sector_number: 100,
                                    pieces: vec![fil_actor_miner_state::[<v $version>]::PieceActivationManifest {
                                        cid: Cid::default(),
                                        size: fvm_shared4::piece::PaddedPieceSize(2048),
                                        verified_allocation_key: None,
                                        notify: vec![],
                                    }],
                                },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            SectorActivationManifestLotusJson {
                                sector_number: self.sector_number,
                                pieces: self.pieces
                                    .into_iter()
                                    .map(|p| p.into_lotus_json()) // delegate
                                    .collect(),
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                sector_number: lotus_json.sector_number,
                                pieces: lotus_json.pieces
                                    .into_iter()
                                    .map(|p| fil_actor_miner_state::[<v $version>]::PieceActivationManifest::from_lotus_json(p)) // delegate
                                    .collect(),
                            }
                        }
                    }
                }

                impl HasLotusJson for fil_actor_miner_state::[<v $version>]::PieceActivationManifest {
                    type LotusJson = PieceActivationManifestLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![(
                            json!({
                                "CID": {"/": "baeaaaaa"},
                                "Size": 2048,
                                "VerifiedAllocationKey": null,
                                "Notify": null
                            }),
                            Self {
                                cid: Cid::default(),
                                size: fvm_shared4::piece::PaddedPieceSize(2048),
                                verified_allocation_key: None,
                                notify: vec![],
                            },
                        )]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        PieceActivationManifestLotusJson {
                            cid: self.cid,
                            size: self.size.0,
                            verified_allocation_key: self.verified_allocation_key.map(|v| VerifiedAllocationKeyLotusJson{
                                id: v.id,
                                client: v.client,
                            }),
                            notify: (!self.notify.is_empty()).then_some(
                                self.notify
                                    .into_iter()
                                    .map(|n| n.into_lotus_json())
                                    .collect_vec()
                            ),
                        }
                    }

                    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                        Self {
                            cid: lotus_json.cid,
                            size: fvm_shared4::piece::PaddedPieceSize(lotus_json.size),
                            verified_allocation_key: lotus_json.verified_allocation_key.map(|v| fil_actor_miner_state::[<v $version>]::VerifiedAllocationKey {
                                client: v.client.into(),
                                id: v.id.into(),
                            }),
                            notify: lotus_json
                                .notify
                                .unwrap_or_default()
                                .into_iter()
                                .map(|n| fil_actor_miner_state::[<v $version>]::DataActivationNotification::from_lotus_json(n))
                                .collect(),
                        }
                    }
                }

                impl HasLotusJson for fil_actor_miner_state::[<v $version>]::DataActivationNotification {
                    type LotusJson = DataActivationNotificationLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![(
                            json!({
                                "Address": "f01234",
                                "Payload": "AQID"
                            }),
                            Self {
                                address: Address::new_id(1234).into(),
                                payload: RawBytes::new(vec![1, 2, 3]),
                            },
                        )]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        DataActivationNotificationLotusJson {
                            address: self.address.into(),
                            payload: self.payload,
                        }
                    }

                    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                        Self {
                            address: lotus_json.address.into(),
                            payload: lotus_json.payload,
                        }
                    }
                }
            }
        )+
    };
}

macro_rules! impl_lotus_json_for_miner_sector_update_manifest {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_miner_sector_update_manifest_ $version>] {
                    use super::*;
                    type T = fil_actor_miner_state::[<v $version>]::SectorUpdateManifest;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }
                    impl HasLotusJson for T {
                        type LotusJson = SectorUpdateManifestLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(
                                json!({
                                    "Sector": 100,
                                    "Deadline": 1,
                                    "Partition": 2,
                                    "NewSealedCID": {"/": "baeaaaaa"},
                                    "Pieces": [
                                        {
                                            "CID": {"/": "baeaaaaa"},
                                            "Size": 2048,
                                            "VerifiedAllocationKey": null,
                                            "Notify": null
                                        }
                                    ]
                                }),
                                Self {
                                    sector: 100,
                                    deadline: 1,
                                    partition: 2,
                                    new_sealed_cid: Cid::default(),
                                    pieces: vec![fil_actor_miner_state::[<v $version>]::PieceActivationManifest {
                                        cid: Cid::default(),
                                        size: fvm_shared4::piece::PaddedPieceSize(2048),
                                        verified_allocation_key: None,
                                        notify: vec![],
                                    }],
                                },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            SectorUpdateManifestLotusJson {
                                sector: self.sector,
                                deadline: self.deadline,
                                partition: self.partition,
                                new_sealed_cid: self.new_sealed_cid,
                                pieces: self.pieces.into_iter().map(|p| p.into_lotus_json()).collect(),
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                sector: lotus_json.sector,
                                deadline: lotus_json.deadline,
                                partition: lotus_json.partition,
                                new_sealed_cid: lotus_json.new_sealed_cid,
                                pieces: lotus_json.pieces.into_iter().map(|p| fil_actor_miner_state::[<v $version>]::PieceActivationManifest::from_lotus_json(p)).collect(),
                            }
                        }
                    }
                }
            }
        )+
    };
}

macro_rules! impl_miner_prove_commit_sector_params {
    ($($version:literal), +) => {
        $(
            paste! {
                mod [<impl_miner_prove_commit_sector_params_ $version>] {
                    use super::*;
                    type T = fil_actor_miner_state::[<v $version>]::ProveCommitSectorParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }
                    impl HasLotusJson for T {
                        type LotusJson = ProveCommitSectorParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(
                                json!({
                                    "SectorNumber": 100,
                                    "Proof": "AQID"
                                }),
                                Self {
                                    sector_number: 100,
                                    proof: vec![1, 2, 3].into(),
                                },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            ProveCommitSectorParamsLotusJson {
                                sector_number: self.sector_number,
                                proof: self.proof.into(),
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                sector_number: lotus_json.sector_number,
                                proof: lotus_json.proof.into(),
                            }
                        }
                    }
                }
            }
        )+
    };
}

impl HasLotusJson for fil_actor_miner_state::v8::ExtendSectorExpirationParams {
    type LotusJson = ExtendSectorExpirationParamsV8LotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!({
                "Extensions": [
                    {
                        "Deadline": 1,
                        "Partition": 2,
                        "Sectors": "AQID",
                        "NewExpiration": 1000
                    }
                ]
            }),
            Self {
                extensions: vec![fil_actor_miner_state::v8::ExpirationExtension {
                    deadline: 1,
                    partition: 2,
                    sectors: UnvalidatedBitField::Unvalidated(vec![1, 2, 3]),
                    new_expiration: 1000,
                }],
            },
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        ExtendSectorExpirationParamsV8LotusJson {
            extensions: self
                .extensions
                .into_iter()
                .map(|e| ExpirationExtensionV8LotusJson {
                    deadline: e.deadline,
                    partition: e.partition,
                    sectors: match e.sectors {
                        UnvalidatedBitField::Validated(bf) => bf.to_bytes(),
                        UnvalidatedBitField::Unvalidated(bytes) => bytes,
                    },
                    new_expiration: e.new_expiration,
                })
                .collect(),
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        Self {
            extensions: lotus_json
                .extensions
                .into_iter()
                .map(|e| fil_actor_miner_state::v8::ExpirationExtension {
                    deadline: e.deadline,
                    partition: e.partition,
                    sectors: UnvalidatedBitField::Unvalidated(e.sectors),
                    new_expiration: e.new_expiration,
                })
                .collect(),
        }
    }
}

macro_rules! impl_miner_extend_sector_expiration_params_v9_onwards {
    ($($version:literal), +) => {
        $(
            paste! {
                mod [<impl_miner_extend_sector_expiration_params_v9_onwards_ $version>] {
                    use super::*;
                    type T = fil_actor_miner_state::[<v $version>]::ExtendSectorExpirationParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }
                    impl HasLotusJson for T {
                        type LotusJson = ExtendSectorExpirationParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(
                                json!({
                                    "Extensions": [
                                        {
                                            "Deadline": 1,
                                            "Partition": 2,
                                            "Sectors": [0],
                                            "NewExpiration": 1000
                                        }
                                    ]
                                }),
                                Self {
                                    extensions: vec![fil_actor_miner_state::[<v $version>]::ExpirationExtension {
                                        deadline: 1,
                                        partition: 2,
                                        sectors: BitField::new(),
                                        new_expiration: 1000,
                                    }],
                                },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            ExtendSectorExpirationParamsLotusJson {
                                extensions: self.extensions.into_iter().map(|e| ExpirationExtensionLotusJson {
                                    deadline: e.deadline,
                                    partition: e.partition,
                                    sectors: e.sectors,
                                    new_expiration: e.new_expiration,
                                }).collect(),
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                extensions: lotus_json.extensions.into_iter().map(|e| fil_actor_miner_state::[<v $version>]::ExpirationExtension {
                                    deadline: e.deadline,
                                    partition: e.partition,
                                    sectors: e.sectors,
                                    new_expiration: e.new_expiration,
                                }).collect(),
                            }
                        }
                    }
                }
            }
        )+
    };
}

macro_rules! impl_miner_confirm_sector_proofs_param_v8_to_v13 {
    ($type_suffix:path: $($version:literal), +) => {
        $(
            paste! {
                mod [<impl_miner_confirm_sector_proofs_param_v8_to_v13_ $version>] {
                    use super::*;
                    type T = fil_actor_miner_state::[<v $version>]::ConfirmSectorProofsParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }
                    impl HasLotusJson for T {
                        type LotusJson = ConfirmSectorProofsParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(
                                json!({
                                    "SectorNumbers": [100, 200, 300],
                                    "RewardSmoothed": {
                                        "PositionEstimate": "1000000000000000000",
                                        "VelocityEstimate": "500000000000000000"
                                    },
                                    "RewardBaselinePower": "1000000000000000000",
                                    "QualityAdjPowerSmoothed": {
                                        "PositionEstimate": "2000000000000000000",
                                        "VelocityEstimate": "1000000000000000000"
                                    }
                                }),
                                Self {
                                    sectors: vec![100, 200, 300],
                                    reward_smoothed: $type_suffix::smooth::FilterEstimate {
                                        position: BigInt::from(1000000000000000000u64),
                                        velocity: BigInt::from(500000000000000000u64),
                                    },
                                    reward_baseline_power: BigInt::from(1000000000000000000u64),
                                    quality_adj_power_smoothed: $type_suffix::smooth::FilterEstimate {
                                        position: BigInt::from(2000000000000000000u64),
                                        velocity: BigInt::from(1000000000000000000u64),
                                    },
                                },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            ConfirmSectorProofsParamsLotusJson {
                                sector_numbers: self.sectors,
                                reward_smoothed: FilterEstimate{
                                    position: self.reward_smoothed.position,
                                    velocity: self.reward_smoothed.velocity,
                                },
                                reward_baseline_power: self.reward_baseline_power,
                                quality_adj_power_smoothed: FilterEstimate{
                                    position: self.quality_adj_power_smoothed.position,
                                    velocity: self.quality_adj_power_smoothed.velocity,
                                },
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                sectors: lotus_json.sector_numbers,
                                reward_smoothed: $type_suffix::smooth::FilterEstimate{
                                    position: lotus_json.reward_smoothed.position,
                                    velocity: lotus_json.reward_smoothed.velocity,
                                },
                                reward_baseline_power: lotus_json.reward_baseline_power,
                                quality_adj_power_smoothed: $type_suffix::smooth::FilterEstimate{
                                    position: lotus_json.quality_adj_power_smoothed.position,
                                    velocity: lotus_json.quality_adj_power_smoothed.velocity,
                                },
                            }
                        }
                    }
                }
            }
        )+
    };
}

impl HasLotusJson for fil_actor_miner_state::v14::ConfirmSectorProofsParams {
    type LotusJson = ConfirmSectorProofsParamsLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!({
                "SectorNumbers": [100, 200, 300],
                "RewardSmoothed": {
                    "PositionEstimate": "1000000000000000000",
                    "VelocityEstimate": "500000000000000000"
                },
                "RewardBaselinePower": "0",
                "QualityAdjPowerSmoothed": {
                    "PositionEstimate": "0",
                    "VelocityEstimate": "0"
                }
            }),
            Self {
                sectors: vec![100, 200, 300],
                reward_smoothed: fil_actors_shared::v14::builtin::reward::smooth::FilterEstimate {
                    position: BigInt::from(1000000000000000000u64),
                    velocity: BigInt::from(500000000000000000u64),
                },
                reward_baseline_power: Default::default(),
                quality_adj_power_smoothed: Default::default(),
            },
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        ConfirmSectorProofsParamsLotusJson {
            sector_numbers: self.sectors,
            reward_smoothed: FilterEstimate {
                position: self.reward_smoothed.position,
                velocity: self.reward_smoothed.velocity,
            },
            reward_baseline_power: self.reward_baseline_power,
            quality_adj_power_smoothed: FilterEstimate {
                position: self.quality_adj_power_smoothed.position,
                velocity: self.quality_adj_power_smoothed.velocity,
            },
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        Self {
            sectors: lotus_json.sector_numbers,
            reward_smoothed: fil_actors_shared::v14::builtin::reward::smooth::FilterEstimate {
                position: lotus_json.reward_smoothed.position,
                velocity: lotus_json.reward_smoothed.velocity,
            },
            reward_baseline_power: Default::default(),
            quality_adj_power_smoothed: Default::default(),
        }
    }
}

macro_rules! impl_miner_deferred_cron_event_params_v14_onwards {
    ($($version:literal), +) => {
        $(
            paste! {
                mod [<impl_miner_deferred_cron_event_params_v14_onwards_ $version>] {
                    use super::*;
                    type T = fil_actor_miner_state::[<v $version>]::DeferredCronEventParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }
                    impl HasLotusJson for T {
                        type LotusJson = DeferredCronEventParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(
                                json!({
                                    "EventPayload": "AQID",
                                    "RewardSmoothed": {
                                        "PositionEstimate": "1000000000000000000",
                                        "VelocityEstimate": "500000000000000000"
                                    },
                                    "QualityAdjPowerSmoothed": {
                                        "PositionEstimate": "2000000000000000000",
                                        "VelocityEstimate": "1000000000000000000"
                                    }
                                }),
                                Self {
                                    event_payload: vec![1, 2, 3],
                                    reward_smoothed: fil_actors_shared::[<v $version>]::builtin::reward::smooth::FilterEstimate {
                                        position: BigInt::from(1000000000000000000u64),
                                        velocity: BigInt::from(500000000000000000u64),
                                    },
                                    quality_adj_power_smoothed: fil_actors_shared::[<v $version>]::builtin::reward::smooth::FilterEstimate {
                                        position: BigInt::from(2000000000000000000u64),
                                        velocity: BigInt::from(1000000000000000000u64),
                                    },
                                },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            DeferredCronEventParamsLotusJson{
                                event_payload: self.event_payload,
                                reward_smoothed: FilterEstimate{
                                    position: self.reward_smoothed.position,
                                    velocity: self.reward_smoothed.velocity,
                                },
                                quality_adj_power_smoothed: FilterEstimate{
                                    position: self.quality_adj_power_smoothed.position,
                                    velocity: self.quality_adj_power_smoothed.velocity,
                                },
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self{
                                event_payload: lotus_json.event_payload,
                                reward_smoothed: fil_actors_shared::[<v $version>]::builtin::reward::smooth::FilterEstimate{
                                    position: lotus_json.reward_smoothed.position,
                                    velocity: lotus_json.reward_smoothed.velocity,
                                },
                                quality_adj_power_smoothed: fil_actors_shared::[<v $version>]::builtin::reward::smooth::FilterEstimate{
                                    position: lotus_json.quality_adj_power_smoothed.position,
                                    velocity: lotus_json.quality_adj_power_smoothed.velocity,
                                },
                            }
                        }
                    }
                }
            }
        )+
    };
}

macro_rules! impl_miner_deferred_cron_event_params_v8_to_v13 {
    ($type_suffix:path: $($version:literal), +) => {
        $(
            paste! {
                mod [<impl_miner_deferred_cron_event_params_v8_to_v13_ $version>] {
                    use super::*;
                    type T = fil_actor_miner_state::[<v $version>]::DeferredCronEventParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }
                    impl HasLotusJson for T {
                        type LotusJson = DeferredCronEventParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(
                                json!({
                                    "EventPayload": "AQID",
                                    "RewardSmoothed": {
                                        "PositionEstimate": "1000000000000000000",
                                        "VelocityEstimate": "500000000000000000"
                                    },
                                    "QualityAdjPowerSmoothed": {
                                        "PositionEstimate": "2000000000000000000",
                                        "VelocityEstimate": "1000000000000000000"
                                    }
                                }),
                                Self {
                                    event_payload: vec![1, 2, 3],
                                    reward_smoothed: $type_suffix::smooth::FilterEstimate {
                                        position: BigInt::from(1000000000000000000u64),
                                        velocity: BigInt::from(500000000000000000u64),
                                    },
                                    quality_adj_power_smoothed: $type_suffix::smooth::FilterEstimate {
                                        position: BigInt::from(2000000000000000000u64),
                                        velocity: BigInt::from(1000000000000000000u64),
                                    },
                                },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            DeferredCronEventParamsLotusJson{
                                event_payload: self.event_payload,
                                reward_smoothed: FilterEstimate{
                                    position: self.reward_smoothed.position,
                                    velocity: self.reward_smoothed.velocity,
                                },
                                quality_adj_power_smoothed: FilterEstimate{
                                    position: self.quality_adj_power_smoothed.position,
                                    velocity: self.quality_adj_power_smoothed.velocity,
                                },
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self{
                                event_payload: lotus_json.event_payload,
                                reward_smoothed: $type_suffix::smooth::FilterEstimate{
                                    position: lotus_json.reward_smoothed.position,
                                    velocity: lotus_json.reward_smoothed.velocity,
                                },
                                quality_adj_power_smoothed: $type_suffix::smooth::FilterEstimate{
                                    position: lotus_json.quality_adj_power_smoothed.position,
                                    velocity: lotus_json.quality_adj_power_smoothed.velocity,
                                },
                            }
                        }
                    }
                }
            }
        )+
    };
}

macro_rules! impl_miner_prove_replica_update_params2 {
    ($type_suffix:path: $($version:literal), +) => {
        $(
            paste! {
                mod [<impl_miner_prove_replica_update_params2_ $version>] {
                    use super::*;
                    type T = fil_actor_miner_state::[<v $version>]::ProveReplicaUpdatesParams2;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }
                    impl HasLotusJson for T {
                        type LotusJson = ProveReplicaUpdatesParams2LotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(
                                json!({
                                    "Updates": [
                                        {
                                            "SectorNumber": 100,
                                            "Deadline": 1,
                                            "Partition": 2,
                                            "NewSealedCid": {"/": "baeaaaaa"},
                                            "NewUnsealedCid": {"/": "baeaaaaa"},
                                            "Deals": [1, 2, 3],
                                            "UpdateProofType": 1,
                                            "ReplicaProof": "AQID"
                                        }
                                    ]
                                }),
                                Self {
                                    updates: vec![fil_actor_miner_state::[<v $version>]::ReplicaUpdate2 {
                                        sector_number: 100,
                                        deadline: 1,
                                        partition: 2,
                                        new_sealed_cid: Cid::default(),
                                        new_unsealed_cid: Cid::default(),
                                        deals: vec![1, 2, 3],
                                        update_proof_type: $type_suffix::sector::RegisteredUpdateProof::from(1),
                                        replica_proof: vec![1, 2, 3],
                                    }],
                                },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            ProveReplicaUpdatesParams2LotusJson {
                                updates: self.updates.into_iter().map(|u| ReplicaUpdate2LotusJson {
                                    sector_number: u.sector_number,
                                    deals: u.deals,
                                    deadline: u.deadline,
                                    partition: u.partition,
                                    new_sealed_cid: u.new_sealed_cid,
                                    update_proof_type: i64::from(u.update_proof_type),
                                    replica_proof: u.replica_proof,
                                    new_unsealed_cid: u.new_unsealed_cid,
                                }).collect(),
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                updates: lotus_json.updates.into_iter().map(|u| fil_actor_miner_state::[<v $version>]::ReplicaUpdate2{
                                    sector_number: u.sector_number,
                                    deadline: u.deadline,
                                    partition: u.partition,
                                    new_sealed_cid: u.new_sealed_cid,
                                    new_unsealed_cid: u.new_unsealed_cid,
                                    deals: u.deals,
                                    update_proof_type: u.update_proof_type.into(),
                                    replica_proof: u.replica_proof,
                                }).collect(),
                            }
                        }
                    }
                }
            }
        )+
    };
}

macro_rules! impl_lotus_json_for_miner_prove_commit_sector_ni_params {
    ($($version:literal), +) => {
        $(
            paste! {
                mod [<impl_miner_prove_commit_sector_ni_params_ $version>] {
                    use super::*;
                    type T = fil_actor_miner_state::[<v $version>]::ProveCommitSectorsNIParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }
                    impl HasLotusJson for T {
                        type LotusJson = ProveCommitSectorsNIParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            let test_cid = Cid::default();
                            vec![(
                                json!({
                                    "Sectors": [
                                        {
                                            "SealingNumber": 100,
                                            "SealerID": 1234,
                                            "SealedCID": {"/": "baeaaaaa"},
                                            "SectorNumber": 200,
                                            "SealRandEpoch": 1000,
                                            "Expiration": 2000
                                        }
                                    ],
                                    "AggregateProof": "AQID",
                                    "SealProofType": 1,
                                    "AggregateProofType": 1,
                                    "ProvingDeadline": 1,
                                    "RequireActivationSuccess": true,
                                }),
                                Self {
                                    sectors: vec![fil_actor_miner_state::[<v $version>]::SectorNIActivationInfo {
                                        sealing_number: 100,
                                        sealer_id: 1234,
                                        sealed_cid: test_cid,
                                        sector_number: 200,
                                        seal_rand_epoch: 1000,
                                        expiration: 2000,
                                    }],
                                    aggregate_proof: RawBytes::new(vec![1, 2, 3]),
                                    seal_proof_type: crate::shim::sector::RegisteredSealProof::from(1).into(),
                                    aggregate_proof_type: fvm_shared4::sector::RegisteredAggregateProof::from(1).into(),
                                    proving_deadline: 1,
                                    require_activation_success: true,
                                },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            ProveCommitSectorsNIParamsLotusJson {
                                sectors: self.sectors.into_iter().map(|s| SectorNIActivationInfoLotusJson{
                                    sealing_number: s.sealing_number,
                                    sealer_id: s.sealer_id,
                                    sealed_cid: s.sealed_cid,
                                    sector_number: s.sector_number,
                                    seal_rand_epoch: s.seal_rand_epoch,
                                    expiration: s.expiration,
                                }).collect(),
                                aggregate_proof: self.aggregate_proof.into(),
                                seal_proof_type: self.seal_proof_type.into(),
                                aggregate_proof_type: i64::from(self.aggregate_proof_type),
                                proving_deadline: self.proving_deadline,
                                require_activation_success: self.require_activation_success,
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                sectors: lotus_json.sectors.into_iter().map(|s| fil_actor_miner_state::[<v $version>]::SectorNIActivationInfo{
                                    sealing_number: s.sealing_number,
                                    sealer_id: s.sealer_id,
                                    sealed_cid: s.sealed_cid,
                                    sector_number: s.sector_number,
                                    seal_rand_epoch: s.seal_rand_epoch,
                                    expiration: s.expiration,
                                }).collect(),
                                aggregate_proof: lotus_json.aggregate_proof.into(),
                                seal_proof_type: crate::shim::sector::RegisteredSealProof::from(lotus_json.seal_proof_type).into(),
                                aggregate_proof_type: fvm_shared4::sector::RegisteredAggregateProof::from(lotus_json.aggregate_proof_type).into(),
                                proving_deadline: lotus_json.proving_deadline,
                                require_activation_success: lotus_json.require_activation_success,
                            }
                        }
                    }
                }
            }
        )+
    };
}

macro_rules! impl_miner_internal_sector_setup_for_preseal_params {
    ($($version:literal), +) => {
        $(
            paste! {
                mod [<impl_miner_internal_sector_setup_for_preseal_params_ $version>] {
                    use super::*;
                    type T = fil_actor_miner_state::[<v $version>]::InternalSectorSetupForPresealParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }
                    impl HasLotusJson for T {
                        type LotusJson = InternalSectorSetupForPresealParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(
                                json!({
                                    "Sectors": [100, 200, 300],
                                    "RewardSmoothed": {
                                        "PositionEstimate": "1000000000000000000",
                                        "VelocityEstimate": "500000000000000000"
                                    },
                                    "RewardBaselinePower": "1000000000000000000",
                                    "QualityAdjPowerSmoothed": {
                                        "PositionEstimate": "2000000000000000000",
                                        "VelocityEstimate": "1000000000000000000"
                                    }
                                }),
                                Self {
                                    sectors: vec![100, 200, 300],
                                    reward_smoothed: fil_actors_shared::[<v $version>]::builtin::reward::smooth::FilterEstimate {
                                        position: BigInt::from(1000000000000000000u64),
                                        velocity: BigInt::from(500000000000000000u64),
                                    },
                                    reward_baseline_power: BigInt::from(1000000000000000000u64),
                                    quality_adj_power_smoothed: fil_actors_shared::[<v $version>]::builtin::reward::smooth::FilterEstimate {
                                        position: BigInt::from(2000000000000000000u64),
                                        velocity: BigInt::from(1000000000000000000u64),
                                    },
                                },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            InternalSectorSetupForPresealParamsLotusJson{
                                sectors: self.sectors,
                                reward_smoothed: FilterEstimate {
                                    position: self.reward_smoothed.position,
                                    velocity: self.reward_smoothed.velocity,
                                },
                                reward_baseline_power: self.reward_baseline_power,
                                quality_adj_power_smoothed: FilterEstimate{
                                    position: self.quality_adj_power_smoothed.position,
                                    velocity: self.quality_adj_power_smoothed.velocity,
                                },
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self{
                                sectors: lotus_json.sectors,
                                reward_smoothed: fil_actors_shared::[<v $version>]::builtin::reward::smooth::FilterEstimate{
                                    position: lotus_json.reward_smoothed.position,
                                    velocity: lotus_json.reward_smoothed.velocity,
                                },
                                reward_baseline_power: lotus_json.reward_baseline_power,
                                quality_adj_power_smoothed: fil_actors_shared::[<v $version>]::builtin::reward::smooth::FilterEstimate{
                                    position: lotus_json.quality_adj_power_smoothed.position,
                                    velocity: lotus_json.quality_adj_power_smoothed.velocity,
                                },
                            }
                        }
                    }
                }
            }
        )+
    };
}

impl_lotus_json_for_miner_constructor_params!(8, 9, 10, 11, 12, 13, 14, 15, 16, 17);
impl_lotus_json_for_miner_change_worker_param!(8, 9, 10, 11, 12, 13, 14, 15, 16, 17);
impl_lotus_json_for_miner_change_owner_address_params!(11, 12, 13, 14, 15, 16, 17);
impl_lotus_json_for_miner_extend_sector_expiration2_params!(9, 10, 11, 12, 13, 14, 15, 16, 17);
impl_lotus_json_for_miner_change_beneficiary_params!(9, 10, 11, 12, 13, 14, 15, 16, 17);
impl_lotus_json_for_miner_declare_faults_recovered_params!(8, 9, 10, 11, 12, 13, 14, 15, 16, 17);
impl_lotus_json_for_miner_dispute_windowed_post_params!(8, 9, 10, 11, 12, 13, 14, 15, 16, 17);
impl_lotus_json_for_recover_declaration_params_v9_and_above!(9, 10, 11, 12, 13, 14, 15, 16, 17);
impl_lotus_json_for_miner_post_partition_v9_and_above!(9, 10, 11, 12, 13, 14, 15, 16, 17);
impl_lotus_json_for_miner_submit_windowed_post_params_v9_and_above!(fvm_shared2: 9);
impl_lotus_json_for_miner_submit_windowed_post_params_v9_and_above!(fvm_shared3: 10, 11);
impl_lotus_json_for_miner_submit_windowed_post_params_v9_and_above!(fvm_shared4: 12, 13, 14, 15, 16, 17);
impl_lotus_json_for_miner_declare_faults_params_v9_and_above!(9, 10, 11, 12, 13, 14, 15, 16, 17);
impl_lotus_json_for_miner_declare_faults_params!(8, 9, 10, 11, 12, 13, 14, 15, 16, 17);
impl_lotus_json_for_miner_termination_declaration_v9_and_above!(9, 10, 11, 12, 13, 14, 15, 16, 17);
impl_lotus_json_for_miner_terminate_sectors_params_v9_and_above!(
    8, 9, 10, 11, 12, 13, 14, 15, 16, 17
);
impl_lotus_json_for_miner_withdraw_balance_params!(8, 9, 10, 11, 12, 13, 14, 15, 16, 17);
impl_lotus_json_for_miner_change_multiaddrs_params!(8, 9, 10, 11, 12, 13, 14, 15, 16, 17);
impl_lotus_json_for_miner_compact_partitions_params!(9, 10, 11, 12, 13, 14, 15, 16, 17);
impl_lotus_json_for_miner_compact_sector_numbers_params!(9, 10, 11, 12, 13, 14, 15, 16, 17);
impl_lotus_json_for_miner_pre_commit_sector_params!(8, 9, 10, 11, 12, 13, 14, 15, 16, 17);
impl_lotus_json_for_miner_pre_commit_sector_and_batch_params!(9, 10, 11, 12, 13, 14, 15, 16, 17);
impl_lotus_json_for_miner_pre_commit_sector_batch2_params!(9, 10, 11, 12, 13, 14, 15, 16, 17);
impl_lotus_json_for_miner_prove_commit_sectors3_params!(13, 14, 15, 16, 17);
impl_lotus_json_for_miner_prove_replica_updates3_params!(13, 14, 15, 16, 17);
impl_lotus_json_for_miner_report_consensus_fault_params!(8, 9, 10, 11, 12, 13, 14, 15, 16, 17);
impl_lotus_json_for_miner_check_sector_proven_params!(8, 9, 10, 11, 12, 13, 14, 15, 16, 17);
impl_lotus_json_for_miner_apply_reward_params!(8, 9, 10, 11, 12, 13, 14, 15, 16, 17);
impl_lotus_json_for_miner_prove_commit_aggregate_params_v9_to_v16!(9, 10, 11, 12, 13, 14, 15, 16);
impl_lotus_json_for_miner_prove_replica_updates_params!(fvm_shared2: 8, 9);
impl_lotus_json_for_miner_prove_replica_updates_params!(fvm_shared3: 10, 11);
impl_lotus_json_for_miner_prove_replica_updates_params!(fvm_shared4: 12, 13, 14, 15, 16);
impl_lotus_json_for_miner_is_controlling_address_param!(10, 11, 12, 13, 14, 15, 16, 17);
impl_lotus_json_for_miner_max_termination_fee_params!(16, 17);
impl_lotus_json_for_miner_change_peer_id_params!(8, 9, 10, 11, 12, 13, 14, 15, 16, 17);
impl_lotus_json_for_miner_sector_activation_manifest!(13, 14, 15, 16, 17);
impl_lotus_json_for_miner_sector_update_manifest!(13, 14, 15, 16, 17);
impl_miner_prove_commit_sector_params!(8, 9, 10, 11, 12, 13, 14, 15, 16, 17);
impl_miner_extend_sector_expiration_params_v9_onwards!(9, 10, 11, 12, 13, 14, 15, 16);
impl_miner_confirm_sector_proofs_param_v8_to_v13!(fvm_shared2: 8, 9);
impl_miner_confirm_sector_proofs_param_v8_to_v13!(fvm_shared3: 10, 11,12, 13);
impl_miner_deferred_cron_event_params_v14_onwards!(14, 15, 16, 17);
impl_miner_deferred_cron_event_params_v8_to_v13!(fvm_shared2: 8, 9);
impl_miner_deferred_cron_event_params_v8_to_v13!(fvm_shared3: 10, 11, 12, 13);
impl_miner_prove_replica_update_params2!(fvm_shared2: 9);
impl_miner_prove_replica_update_params2!(fvm_shared3: 10, 11);
impl_miner_prove_replica_update_params2!(fvm_shared4: 12);
impl_lotus_json_for_miner_prove_commit_sector_ni_params!(14, 15, 16, 17);
impl_miner_internal_sector_setup_for_preseal_params!(14, 15, 16, 17);
