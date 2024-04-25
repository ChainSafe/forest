// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

// TODO(aatifsyed): https://github.com/ChainSafe/forest/issues/4032
//                  this should move to src/lotus_json
impl HasLotusJson for MinerInfo {
    type LotusJson = MinerInfoLotusJson;
    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![]
    }
    fn into_lotus_json(self) -> Self::LotusJson {
        MinerInfoLotusJson {
            owner: self.owner.into(),
            worker: self.worker.into(),
            new_worker: AddressOrEmpty(self.new_worker.map(|addr| addr.into())),
            control_addresses: self
                .control_addresses
                .into_iter()
                .map(|a| a.into())
                .collect(),
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
            beneficiary: self.beneficiary.into(),
            beneficiary_term: self.beneficiary_term,
            pending_beneficiary_term: self.pending_beneficiary_term,
        }
    }
    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        MinerInfo {
            owner: lotus_json.owner.into(),
            worker: lotus_json.worker.into(),
            new_worker: lotus_json.new_worker.0.map(|addr| addr.into()),
            control_addresses: lotus_json
                .control_addresses
                .into_iter()
                .map(|a| a.into())
                .collect(),
            worker_change_epoch: lotus_json.worker_change_epoch,
            peer_id: lotus_json.peer_id.map_or_else(Vec::new, |s| s.into_bytes()),
            multiaddrs: lotus_json.multiaddrs.into_iter().map(BytesDe).collect(),
            window_post_proof_type: lotus_json.window_po_st_proof_type,
            sector_size: lotus_json.sector_size,
            window_post_partition_sectors: lotus_json.window_po_st_partition_sectors,
            consensus_fault_elapsed: lotus_json.consensus_fault_elapsed,
            // Ignore this field as it is never set on Lotus side.
            pending_owner_address: None,
            beneficiary: lotus_json.beneficiary.into(),
            beneficiary_term: lotus_json.beneficiary_term,
            pending_beneficiary_term: lotus_json.pending_beneficiary_term,
        }
    }
}

// TODO(aatifsyed): https://github.com/ChainSafe/forest/issues/4032
//                  this should move to src/lotus_json
impl HasLotusJson for MinerPower {
    type LotusJson = MinerPowerLotusJson;
    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![]
    }
    fn into_lotus_json(self) -> Self::LotusJson {
        MinerPowerLotusJson {
            miner_power: self.miner_power,
            total_power: self.total_power,
            has_min_power: self.has_min_power,
        }
    }
    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        MinerPower {
            miner_power: lotus_json.miner_power,
            total_power: lotus_json.total_power,
            has_min_power: lotus_json.has_min_power,
        }
    }
}
