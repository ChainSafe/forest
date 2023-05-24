// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::multihash::Code::Blake2b256;
use fil_actor_miner_state::{
    v10::{MinerInfo as MinerInfoV10, State as MinerStateV10},
    v11::{MinerInfo as MinerInfoV11, State as MinerStateV11},
};
use forest_shim::sector::convert_window_post_proof_v1_to_v1p1;
use forest_utils::db::BlockstoreExt;
use fvm_ipld_blockstore::Blockstore;

use crate::common::{TypeMigration, TypeMigrator};

impl TypeMigration<MinerStateV10, MinerStateV11> for TypeMigrator {
    fn migrate_type(from: MinerStateV10, store: &impl Blockstore) -> anyhow::Result<MinerStateV11> {
        let in_info: MinerInfoV10 = store
            .get_obj(&from.info)?
            .ok_or_else(|| anyhow::anyhow!("Miner info: could not read v10 state"))?;

        let out_proof_type = convert_window_post_proof_v1_to_v1p1(in_info.window_post_proof_type)
            .map_err(|e| anyhow::anyhow!(e))?;

        let out_info = MinerInfoV11 {
            owner: in_info.owner,
            worker: in_info.worker,
            control_addresses: in_info.control_addresses,
            pending_worker_key: in_info.pending_worker_key.map(|key| {
                fil_actor_miner_state::v11::WorkerKeyChange {
                    new_worker: key.new_worker,
                    effective_at: key.effective_at,
                }
            }),
            peer_id: in_info.peer_id,
            multi_address: in_info.multi_address,
            window_post_proof_type: out_proof_type,
            sector_size: in_info.sector_size,
            window_post_partition_sectors: in_info.window_post_partition_sectors,
            consensus_fault_elapsed: in_info.consensus_fault_elapsed,
            pending_owner_address: in_info.pending_owner_address,
            beneficiary: in_info.beneficiary,
            beneficiary_term: fil_actor_miner_state::v11::BeneficiaryTerm {
                quota: in_info.beneficiary_term.quota,
                used_quota: in_info.beneficiary_term.used_quota,
                expiration: in_info.beneficiary_term.expiration,
            },
            pending_beneficiary_term: in_info.pending_beneficiary_term.map(|term| {
                fil_actor_miner_state::v11::PendingBeneficiaryChange {
                    new_beneficiary: term.new_beneficiary,
                    new_quota: term.new_quota,
                    new_expiration: term.new_expiration,
                    approved_by_beneficiary: term.approved_by_beneficiary,
                    approved_by_nominee: term.approved_by_nominee,
                }
            }),
        };

        let out_info_cid = store.put_obj(&out_info, Blake2b256)?;

        let out_state = MinerStateV11 {
            info: out_info_cid,
            pre_commit_deposits: from.pre_commit_deposits,
            locked_funds: from.locked_funds,
            vesting_funds: from.vesting_funds,
            fee_debt: from.fee_debt,
            initial_pledge: from.initial_pledge,
            pre_committed_sectors: from.pre_committed_sectors,
            pre_committed_sectors_cleanup: from.pre_committed_sectors_cleanup,
            allocated_sectors: from.allocated_sectors,
            sectors: from.sectors,
            proving_period_start: from.proving_period_start,
            current_deadline: from.current_deadline,
            deadlines: from.deadlines,
            early_terminations: from.early_terminations,
            deadline_cron_active: from.deadline_cron_active,
        };

        Ok(out_state)
    }
}
