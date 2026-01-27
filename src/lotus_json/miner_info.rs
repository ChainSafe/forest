// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

use crate::shim::actors::miner::MinerInfo;
use crate::{
    rpc::types::AddressOrEmpty,
    shim::{address::Address, clock::ChainEpoch, sector::SectorSize},
};
use fil_actor_miner_state::v12::{BeneficiaryTerm, PendingBeneficiaryChange};
use fvm_ipld_encoding::BytesDe;
use libp2p::PeerId;

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
#[schemars(rename = "MinerInfo")]
pub struct MinerInfoLotusJson {
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub owner: Address,
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub worker: Address,
    #[schemars(with = "LotusJson<Option<Address>>")]
    pub new_worker: AddressOrEmpty,
    #[schemars(with = "LotusJson<Vec<Address>>")]
    #[serde(with = "crate::lotus_json")]
    pub control_addresses: Vec<Address>, // Must all be ID addresses.
    pub worker_change_epoch: ChainEpoch,
    #[schemars(with = "LotusJson<Option<String>>")]
    #[serde(with = "crate::lotus_json")]
    pub peer_id: Option<String>,
    #[schemars(with = "LotusJson<Vec<Vec<u8>>>")]
    #[serde(with = "crate::lotus_json")]
    pub multiaddrs: Vec<Vec<u8>>,
    #[schemars(with = "String")]
    pub window_po_st_proof_type: fvm_shared2::sector::RegisteredPoStProof,
    #[schemars(with = "LotusJson<SectorSize>")]
    #[serde(with = "crate::lotus_json")]
    pub sector_size: SectorSize,
    pub window_po_st_partition_sectors: u64,
    pub consensus_fault_elapsed: ChainEpoch,
    #[schemars(with = "LotusJson<Option<Address>>")]
    #[serde(with = "crate::lotus_json")]
    pub pending_owner_address: Option<Address>,
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub beneficiary: Address,
    #[schemars(with = "LotusJson<BeneficiaryTerm>")]
    #[serde(with = "crate::lotus_json")]
    pub beneficiary_term: BeneficiaryTerm,
    #[schemars(with = "LotusJson<Option<PendingBeneficiaryChange>>")]
    #[serde(with = "crate::lotus_json")]
    pub pending_beneficiary_term: Option<PendingBeneficiaryChange>,
}

impl HasLotusJson for MinerInfo {
    type LotusJson = MinerInfoLotusJson;
    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!({
                "Beneficiary": "f00",
                "BeneficiaryTerm": {
                    "Expiration": 0,
                    "Quota": "0",
                    "UsedQuota": "0"
                },
                "ConsensusFaultElapsed": 0,
                "ControlAddresses": null,
                "Multiaddrs": null,
                "NewWorker": "<empty>",
                "Owner": "f00",
                "PeerId": null,
                "PendingBeneficiaryTerm": null,
                "PendingOwnerAddress": null,
                "SectorSize": 2048,
                "WindowPoStPartitionSectors": 0,
                "WindowPoStProofType": 0,
                "Worker": "f00",
                "WorkerChangeEpoch": 0
            }),
            Self {
                owner: Address::default(),
                worker: Address::default(),
                new_worker: Default::default(),
                control_addresses: Default::default(),
                worker_change_epoch: Default::default(),
                peer_id: Default::default(),
                multiaddrs: Default::default(),
                window_post_proof_type:
                    fvm_shared2::sector::RegisteredPoStProof::StackedDRGWinning2KiBV1,
                sector_size: crate::shim::sector::SectorSize::_2KiB,
                window_post_partition_sectors: Default::default(),
                consensus_fault_elapsed: Default::default(),
                pending_owner_address: Default::default(),
                beneficiary: Address::default(),
                beneficiary_term: Default::default(),
                pending_beneficiary_term: Default::default(),
            },
        )]
    }
    fn into_lotus_json(self) -> Self::LotusJson {
        MinerInfoLotusJson {
            owner: self.owner,
            worker: self.worker,
            new_worker: AddressOrEmpty(self.new_worker),
            control_addresses: self.control_addresses,
            worker_change_epoch: self.worker_change_epoch,
            peer_id: PeerId::try_from(self.peer_id).map(|id| id.to_base58()).ok(),
            multiaddrs: self.multiaddrs.into_iter().map(|addr| addr.0).collect(),
            window_po_st_proof_type: self.window_post_proof_type,
            sector_size: self.sector_size,
            window_po_st_partition_sectors: self.window_post_partition_sectors,
            consensus_fault_elapsed: self.consensus_fault_elapsed,
            // NOTE: In Lotus this field is never set for any of the versions, so we have to ignore
            // it too.
            // See: <https://github.com/filecoin-project/lotus/blob/b6a77dfafcf0110e95840fca15a775ed663836d8/chain/actors/builtin/miner/v12.go#L370>.
            pending_owner_address: None,
            beneficiary: self.beneficiary,
            beneficiary_term: self.beneficiary_term,
            pending_beneficiary_term: self.pending_beneficiary_term,
        }
    }
    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        MinerInfo {
            owner: lotus_json.owner,
            worker: lotus_json.worker,
            new_worker: lotus_json.new_worker.0,
            control_addresses: lotus_json.control_addresses,
            worker_change_epoch: lotus_json.worker_change_epoch,
            peer_id: lotus_json.peer_id.map_or_else(Vec::new, |s| s.into_bytes()),
            multiaddrs: lotus_json.multiaddrs.into_iter().map(BytesDe).collect(),
            window_post_proof_type: lotus_json.window_po_st_proof_type,
            sector_size: lotus_json.sector_size,
            window_post_partition_sectors: lotus_json.window_po_st_partition_sectors,
            consensus_fault_elapsed: lotus_json.consensus_fault_elapsed,
            // Ignore this field as it is never set on Lotus side.
            pending_owner_address: None,
            beneficiary: lotus_json.beneficiary,
            beneficiary_term: lotus_json.beneficiary_term,
            pending_beneficiary_term: lotus_json.pending_beneficiary_term,
        }
    }
}

#[test]
fn snapshots() {
    assert_all_snapshots::<MinerInfo>();
}
