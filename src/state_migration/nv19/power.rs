// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! This module contains the migration logic for the `NV19` upgrade for the
//! Power actor.

use std::sync::Arc;

use crate::shim::sector::convert_window_post_proof_v1_to_v1p1;
use crate::utils::db::CborStoreExt;
use cid::Cid;
use fil_actor_power_state::{
    v10::{Claim as ClaimV10, State as StateV10},
    v11::{Claim as ClaimV11, State as StateV11},
};
use fil_actors_shared::v11::{
    builtin::HAMT_BIT_WIDTH, make_empty_map, make_map_with_root_and_bitwidth,
};
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::CborStore;

use crate::state_migration::common::{ActorMigration, ActorMigrationInput, ActorMigrationOutput};

pub struct PowerMigrator(Cid);

pub(in crate::state_migration) fn power_migrator<BS: Blockstore + Clone + Send + Sync>(
    cid: Cid,
) -> Arc<dyn ActorMigration<BS> + Send + Sync> {
    Arc::new(PowerMigrator(cid))
}

// original golang code: https://github.com/filecoin-project/go-state-types/blob/master/builtin/v11/migration/power.go
impl<BS: Blockstore + Clone + Send + Sync> ActorMigration<BS> for PowerMigrator {
    fn migrate_state(
        &self,
        store: BS,
        input: ActorMigrationInput,
    ) -> anyhow::Result<Option<ActorMigrationOutput>> {
        let in_state: StateV10 = store
            .get_cbor(&input.head)?
            .ok_or_else(|| anyhow::anyhow!("Power actor: could not read v10 state"))?;

        let in_claims = make_map_with_root_and_bitwidth(&in_state.claims, &store, HAMT_BIT_WIDTH)?;

        let empty_claims = make_empty_map::<BS, ()>(&store, HAMT_BIT_WIDTH).flush()?;

        let mut out_claims =
            make_map_with_root_and_bitwidth(&empty_claims, &store, HAMT_BIT_WIDTH)?;

        in_claims.for_each(|key, claim: &ClaimV10| {
            let new_proof_type = convert_window_post_proof_v1_to_v1p1(claim.window_post_proof_type)
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            let out_claim = ClaimV11 {
                window_post_proof_type: new_proof_type,
                raw_byte_power: claim.raw_byte_power.clone(),
                quality_adj_power: claim.quality_adj_power.clone(),
            };
            out_claims.set(key.to_owned(), out_claim)?;
            Ok(())
        })?;

        let out_claims_root = out_claims.flush()?;

        let out_state = StateV11 {
            total_raw_byte_power: in_state.total_raw_byte_power,
            total_bytes_committed: in_state.total_bytes_committed,
            total_quality_adj_power: in_state.total_quality_adj_power,
            total_qa_bytes_committed: in_state.total_qa_bytes_committed,
            total_pledge_collateral: in_state.total_pledge_collateral,
            this_epoch_raw_byte_power: in_state.this_epoch_raw_byte_power,
            this_epoch_quality_adj_power: in_state.this_epoch_quality_adj_power,
            this_epoch_pledge_collateral: in_state.this_epoch_pledge_collateral,
            this_epoch_qa_power_smoothed: in_state.this_epoch_qa_power_smoothed,
            miner_count: in_state.miner_count,
            miner_above_min_power_count: in_state.miner_above_min_power_count,
            cron_event_queue: in_state.cron_event_queue,
            first_cron_epoch: in_state.first_cron_epoch,
            claims: out_claims_root,
            proof_validation_batch: in_state.proof_validation_batch,
        };

        let new_head = store.put_cbor_default(&out_state)?;

        Ok(Some(ActorMigrationOutput {
            new_code_cid: self.0,
            new_head,
        }))
    }
}
