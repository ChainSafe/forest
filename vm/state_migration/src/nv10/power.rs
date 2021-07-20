// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{ActorMigration, ActorMigrationInput};
use crate::{MigrationError, MigrationOutput, MigrationResult};
use actor_interface::actorv2::power::State as V2_PowerState;
use actor_interface::actorv3;
use actor_interface::actorv3::power::State as V3_PowerState;
use actor_interface::actorv3::smooth::FilterEstimate;
use async_std::sync::Arc;
use cid::Cid;
use cid::Code;
use ipld_blockstore::BlockStore;

use crate::nv10::util::migrate_hamt_amt_raw;
use actor::make_empty_map;
use actor::power::{Claim, CronEvent, CRON_QUEUE_AMT_BITWIDTH, CRON_QUEUE_HAMT_BITWIDTH};
use actor_interface::actorv3::power::PROOF_VALIDATION_BATCH_AMT_BITWIDTH;
use actor_interface::ActorVersion;
use actor_interface::Map as Map2;
use fil_types::{SealVerifyInfo, HAMT_BIT_WIDTH};
use forest_hash_utils::BytesKey;
use serde::{de::DeserializeOwned, Serialize};

pub struct PowerMigrator(Cid);

// each actor's state migration is read from blockstore, changes state tree, and writes back to the blocstore.
impl<BS: BlockStore + Send + Sync> ActorMigration<BS> for PowerMigrator {
    fn migrate_state(
        &self,
        store: Arc<BS>,
        input: ActorMigrationInput,
    ) -> MigrationResult<MigrationOutput> {
        let v2_in_state: V2_PowerState = store
            .get(&input.head)
            .map_err(|e| MigrationError::BlockStoreRead(e.to_string()))?
            .ok_or(MigrationError::StateNotFound)?;

        // HAMT[addr.Address]abi.ActorID
        let proof_validation_batch_out = if let Some(batch) = v2_in_state.proof_validation_batch
        {
            Some(migrate_hamt_amt_raw::<_, SealVerifyInfo>(
                &*store,
                batch,
                HAMT_BIT_WIDTH,
                PROOF_VALIDATION_BATCH_AMT_BITWIDTH as u32,
            )?)
        } else {
            None
        };

        let claims_out = self.migrate_claims::<_, Claim>(&*store, v2_in_state.claims)?;

        let cron_event_queue_out = migrate_hamt_amt_raw::<_, CronEvent>(
            &*store,
            v2_in_state.cron_event_queue,
            CRON_QUEUE_HAMT_BITWIDTH,
            CRON_QUEUE_AMT_BITWIDTH as u32,
        )?;

        let v3_filter_estimate = FilterEstimate {
            position: v2_in_state.this_epoch_qa_power_smoothed.position,
            velocity: v2_in_state.this_epoch_qa_power_smoothed.velocity,
        };

        let out_state = V3_PowerState {
            total_raw_byte_power: v2_in_state.total_raw_byte_power,
            total_bytes_committed: v2_in_state.total_bytes_committed,
            total_quality_adj_power: v2_in_state.total_quality_adj_power,
            total_qa_bytes_committed: v2_in_state.total_qa_bytes_committed,
            total_pledge_collateral: v2_in_state.total_pledge_collateral,
            this_epoch_raw_byte_power: v2_in_state.this_epoch_raw_byte_power,
            this_epoch_quality_adj_power: v2_in_state.this_epoch_quality_adj_power,
            this_epoch_pledge_collateral: v2_in_state.this_epoch_pledge_collateral,
            this_epoch_qa_power_smoothed: v3_filter_estimate,
            miner_count: v2_in_state.miner_count,
            miner_above_min_power_count: v2_in_state.miner_above_min_power_count,
            cron_event_queue: cron_event_queue_out,
            first_cron_epoch: v2_in_state.first_cron_epoch,
            claims: claims_out,
            proof_validation_batch: proof_validation_batch_out,
        };

        let new_head = store
            .put(&out_state, Code::Blake2b256)
            .map_err(|e| MigrationError::BlockStoreWrite(e.to_string()))?;

        Ok(MigrationOutput {
            new_code_cid: *actorv3::POWER_ACTOR_CODE_ID,
            new_head,
        })
    }
}

impl PowerMigrator {
    fn migrate_claims<BS: BlockStore, V: Clone + Serialize + PartialEq + DeserializeOwned>(
        &self,
        store: &BS,
        root: Cid,
    ) -> MigrationResult<Cid> {
        let in_claims = Map2::load(&root, store, ActorVersion::V2)
            .map_err(|e| MigrationError::HAMTLoad(e.to_string()))?;

        let mut out_claims = make_empty_map::<_, Claim>(store, HAMT_BIT_WIDTH);

        in_claims
            .for_each(|k: &BytesKey, v: &Claim| {
                let post_proof = v.window_post_proof_type;
                let out_claim = Claim {
                    window_post_proof_type: post_proof,
                    raw_byte_power: v.raw_byte_power.clone(),
                    quality_adj_power: v.quality_adj_power.clone(),
                };

                out_claims.set(k.clone(), out_claim)?;
                Ok(())
            })
            .map_err(|_| MigrationError::Other)?;

        Ok(out_claims
            .flush()
            .map_err(|e| MigrationError::FlushFailed(e.to_string()))?)
    }
}
