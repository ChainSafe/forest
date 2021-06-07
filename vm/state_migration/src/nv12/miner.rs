// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! This module implements the miner actor state migration for network version 12 upgrade.

use crate::{ActorMigration, ActorMigrationInput};
use crate::{MigrationError, MigrationOutput, MigrationResult};
use actor_interface::actorv3::miner::State as V3State;
use actor_interface::actorv4::miner::State as V4State;
use async_std::sync::Arc;
use cid::Cid;
use cid::Code::Blake2b256;
use ipld_blockstore::BlockStore;

pub struct MinerMigrator(Cid);

pub fn miner_migrator_v4<BS: BlockStore + Send + Sync>(
    cid: Cid,
) -> Arc<dyn ActorMigration<BS> + Send + Sync> {
    Arc::new(MinerMigrator(cid))
}

// each actor's state migration is read from blockstore, changes state tree, and writes back to the blocstore.
impl<BS: BlockStore + Send + Sync> ActorMigration<BS> for MinerMigrator {
    fn migrate_state(
        &self,
        store: Arc<BS>,
        input: ActorMigrationInput,
    ) -> MigrationResult<MigrationOutput> {
        let v3_state: Option<V3State> = store
            .get(&input.head)
            .map_err(|e| MigrationError::BlockStoreRead(e.to_string()))?;
        let in_state: V3State = v3_state.ok_or_else(|| {
            MigrationError::BlockStoreRead("Miner actor: could not read v3 state".to_string())
        })?;

        let out_state = V4State {
            info: in_state.info,
            pre_commit_deposits: in_state.pre_commit_deposits,
            locked_funds: in_state.locked_funds,
            vesting_funds: in_state.vesting_funds,
            fee_debt: in_state.fee_debt,
            initial_pledge: in_state.initial_pledge,
            pre_committed_sectors: in_state.pre_committed_sectors,
            pre_committed_sectors_expiry: in_state.pre_committed_sectors_expiry,
            allocated_sectors: in_state.allocated_sectors,
            sectors: in_state.sectors,
            proving_period_start: in_state.proving_period_start,
            current_deadline: in_state.current_deadline as usize,
            deadlines: in_state.deadlines,
            early_terminations: in_state.early_terminations,
            deadline_cron_active: true,
        };

        let new_head = store
            .put(&out_state, Blake2b256)
            .map_err(|e| MigrationError::BlockStoreWrite(e.to_string()))?;

        Ok(MigrationOutput {
            new_code_cid: self.0,
            new_head,
        })
    }
}
