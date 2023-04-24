// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! This module contains the migration logic for the `NV19` upgrade for the
//! Miner actor.

use std::sync::Arc;

use cid::{multihash::Code::Blake2b256, Cid};
use fil_actor_miner_v10::{MinerInfo, State as StateV10};
use fil_actor_miner_v11::{MinerInfo as MinerInfoV11, State as StateV11};
use forest_shim::sector::convert_window_post_proof_v1_to_v1p1;
use forest_utils::db::BlockstoreExt;
use fvm_ipld_blockstore::Blockstore;

use crate::common::{ActorMigration, ActorMigrationInput, ActorMigrationOutput};

pub struct MinerMigrator(Cid);

pub(crate) fn miner_migrator<BS: Blockstore + Clone + Send + Sync>(
    cid: Cid,
) -> Arc<dyn ActorMigration<BS> + Send + Sync> {
    Arc::new(MinerMigrator(cid))
}

impl<BS: Blockstore + Clone + Send + Sync> ActorMigration<BS> for MinerMigrator {
    fn migrate_state(
        &self,
        store: BS,
        input: ActorMigrationInput,
    ) -> anyhow::Result<ActorMigrationOutput> {
        let in_state: StateV10 = store
            .get_obj(&input.head)?
            .ok_or_else(|| anyhow::anyhow!("Miner actor: could not read v10 state"))?;

        let in_info: MinerInfo = store
            .get_obj(&in_state.info)?
            .ok_or_else(|| anyhow::anyhow!("Miner info: could not read v10 state"))?;

        let out_proof_type = convert_window_post_proof_v1_to_v1p1(in_info.window_post_proof_type)
            .map_err(|e| anyhow::anyhow!(e))?;

        let out_info = MinerInfoV11 {
            owner: in_info.owner,
            worker: in_info.worker,
            control_addresses: in_info.control_addresses,
            pending_worker_key: in_info.pending_worker_key.map(|key| {
                fil_actor_miner_v11::WorkerKeyChange {
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
            beneficiary_term: fil_actor_miner_v11::BeneficiaryTerm {
                quota: in_info.beneficiary_term.quota,
                used_quota: in_info.beneficiary_term.used_quota,
                expiration: in_info.beneficiary_term.expiration,
            },
            pending_beneficiary_term: in_info.pending_beneficiary_term.map(|term| {
                fil_actor_miner_v11::PendingBeneficiaryChange {
                    new_beneficiary: term.new_beneficiary,
                    new_quota: term.new_quota,
                    new_expiration: term.new_expiration,
                    approved_by_beneficiary: term.approved_by_beneficiary,
                    approved_by_nominee: term.approved_by_nominee,
                }
            }),
        };

        let out_info_cid = store.put_obj(&out_info, Blake2b256)?;

        let out_state = StateV11 {
            info: out_info_cid,
            pre_commit_deposits: in_state.pre_commit_deposits,
            locked_funds: in_state.locked_funds,
            vesting_funds: in_state.vesting_funds,
            fee_debt: in_state.fee_debt,
            initial_pledge: in_state.initial_pledge,
            pre_committed_sectors: in_state.pre_committed_sectors,
            pre_committed_sectors_cleanup: in_state.pre_committed_sectors_cleanup,
            allocated_sectors: in_state.allocated_sectors,
            sectors: in_state.sectors,
            proving_period_start: in_state.proving_period_start,
            current_deadline: in_state.current_deadline,
            deadlines: in_state.deadlines,
            early_terminations: in_state.early_terminations,
            deadline_cron_active: in_state.deadline_cron_active,
        };

        let new_head = store.put_obj(&out_state, Blake2b256)?;

        Ok(ActorMigrationOutput {
            new_code_cid: self.0,
            new_head,
        })
    }
}
