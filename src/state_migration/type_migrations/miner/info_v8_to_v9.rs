// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fil_actor_miner_state::{v8::MinerInfo as MinerInfoV8, v9::MinerInfo as MinerInfoV9};
use fvm_ipld_blockstore::Blockstore;

use super::super::super::common::{TypeMigration, TypeMigrator};

impl TypeMigration<MinerInfoV8, MinerInfoV9> for TypeMigrator {
    fn migrate_type(from: MinerInfoV8, _: &impl Blockstore) -> anyhow::Result<MinerInfoV9> {
        // https://github.com/filecoin-project/go-state-types/blob/master/builtin/v9/migration/miner.go#L133
        let out_info = MinerInfoV9 {
            owner: from.owner,
            worker: from.worker,
            control_addresses: from.control_addresses,
            pending_worker_key: from.pending_worker_key.map(|key| {
                fil_actor_miner_state::v9::WorkerKeyChange {
                    new_worker: key.new_worker,
                    effective_at: key.effective_at,
                }
            }),
            peer_id: from.peer_id,
            multi_address: from.multi_address,
            window_post_proof_type: from.window_post_proof_type,
            sector_size: from.sector_size,
            window_post_partition_sectors: from.window_post_partition_sectors,
            consensus_fault_elapsed: from.consensus_fault_elapsed,
            pending_owner_address: from.pending_owner_address,
            beneficiary: from.owner,
            beneficiary_term: fil_actor_miner_state::v9::BeneficiaryTerm {
                quota: Default::default(),
                used_quota: Default::default(),
                expiration: 0,
            },
            pending_beneficiary_term: None,
        };

        Ok(out_info)
    }
}
