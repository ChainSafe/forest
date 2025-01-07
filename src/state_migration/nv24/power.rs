// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! This module contains the migration logic for the `NV24` upgrade for the
//! Power actor.

use crate::shim::clock::ChainEpoch;
use crate::state_migration::common::{ActorMigration, ActorMigrationInput, ActorMigrationOutput};
use crate::utils::db::CborStoreExt as _;
use cid::Cid;
use fil_actor_power_state::{v14::State as StateV14, v15::State as StateV15};
use fil_actors_shared::v15::builtin::reward::smooth::FilterEstimate as FilterEstimateV15;
use fvm_ipld_blockstore::Blockstore;
use std::sync::Arc;

pub struct PowerMigrator {
    new_code_cid: Cid,
    tuktuk_epoch: ChainEpoch,
    ramp_duration_epochs: u64,
}

pub(in crate::state_migration) fn power_migrator<BS: Blockstore>(
    cid: Cid,
    tuktuk_epoch: ChainEpoch,
    ramp_duration_epochs: u64,
) -> Arc<dyn ActorMigration<BS> + Send + Sync> {
    Arc::new(PowerMigrator {
        new_code_cid: cid,
        tuktuk_epoch,
        ramp_duration_epochs,
    })
}

// The v15 actor is identical to v14, except for the addition of the `ramp_start_epoch`
// and `ramp_duration_epochs` fields.
impl<BS: Blockstore> ActorMigration<BS> for PowerMigrator {
    fn migrate_state(
        &self,
        store: &BS,
        input: ActorMigrationInput,
    ) -> anyhow::Result<Option<ActorMigrationOutput>> {
        let in_state: StateV14 = store.get_cbor_required(&input.head)?;

        let out_state = StateV15 {
            total_raw_byte_power: in_state.total_raw_byte_power,
            total_bytes_committed: in_state.total_bytes_committed,
            total_quality_adj_power: in_state.total_quality_adj_power,
            total_qa_bytes_committed: in_state.total_qa_bytes_committed,
            total_pledge_collateral: in_state.total_pledge_collateral,
            this_epoch_raw_byte_power: in_state.this_epoch_raw_byte_power,
            this_epoch_quality_adj_power: in_state.this_epoch_quality_adj_power,
            this_epoch_pledge_collateral: in_state.this_epoch_pledge_collateral,
            this_epoch_qa_power_smoothed: FilterEstimateV15 {
                position: in_state.this_epoch_qa_power_smoothed.position,
                velocity: in_state.this_epoch_qa_power_smoothed.velocity,
            },
            miner_count: in_state.miner_count,
            miner_above_min_power_count: in_state.miner_above_min_power_count,
            ramp_start_epoch: self.tuktuk_epoch,
            ramp_duration_epochs: self.ramp_duration_epochs,
            cron_event_queue: in_state.cron_event_queue,
            first_cron_epoch: in_state.first_cron_epoch,
            claims: in_state.claims,
            proof_validation_batch: in_state.proof_validation_batch,
        };

        let new_head = store.put_cbor_default(&out_state)?;

        Ok(Some(ActorMigrationOutput {
            new_code_cid: self.new_code_cid,
            new_head,
        }))
    }
}
