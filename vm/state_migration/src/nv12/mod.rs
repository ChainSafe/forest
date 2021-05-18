// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! This module implements network version 12 or actorv4 state migration
//! Please read https://filecoin.io/blog/posts/filecoin-network-v12/
//! to learn more about network version 12 migration.
//! This is more or less a direct port of the state migration
//! implemented in lotus' specs-actors library.

pub mod miner;

use crate::nil_migrator_v4;
use crate::{ActorMigration, MigrationError, MigrationJob, MigrationResult};
use actor_interface::{actorv3, actorv4};
use async_std::task;
use cid::Cid;
use clock::ChainEpoch;
use fil_types::StateTreeVersion;
use futures::stream::FuturesOrdered;
use futures::StreamExt;
use ipld_blockstore::BlockStore;
use miner::miner_migrator_v4;
use state_tree::StateTree;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

const ACTORS_COUNT: usize = 11;

pub fn migrate_state_tree<'db, BS: BlockStore>(
    store: &'db BS,
    actors_root_in: Cid,
    prior_epoch: ChainEpoch,
) -> MigrationResult<Cid> {
    let mut jobs = FuturesOrdered::new();

    // Maps prior version code CIDs to migration functions.
    let mut migrations: HashMap<Cid, Rc<dyn ActorMigration<BS>>> =
        HashMap::with_capacity(ACTORS_COUNT);
    migrations.insert(
        *actorv3::ACCOUNT_ACTOR_CODE_ID,
        nil_migrator_v4(*actorv4::ACCOUNT_ACTOR_CODE_ID),
    );
    migrations.insert(
        *actorv3::CRON_ACTOR_CODE_ID,
        nil_migrator_v4(*actorv4::CRON_ACTOR_CODE_ID),
    );
    migrations.insert(
        *actorv3::INIT_ACTOR_CODE_ID,
        nil_migrator_v4(*actorv4::INIT_ACTOR_CODE_ID),
    );
    migrations.insert(
        *actorv3::MULTISIG_ACTOR_CODE_ID,
        nil_migrator_v4(*actorv4::MULTISIG_ACTOR_CODE_ID),
    );
    migrations.insert(
        *actorv3::PAYCH_ACTOR_CODE_ID,
        nil_migrator_v4(*actorv4::PAYCH_ACTOR_CODE_ID),
    );
    migrations.insert(
        *actorv3::REWARD_ACTOR_CODE_ID,
        nil_migrator_v4(*actorv4::REWARD_ACTOR_CODE_ID),
    );
    migrations.insert(
        *actorv3::MARKET_ACTOR_CODE_ID,
        nil_migrator_v4(*actorv4::MARKET_ACTOR_CODE_ID),
    );
    migrations.insert(
        *actorv3::MINER_ACTOR_CODE_ID,
        miner_migrator_v4(*actorv4::MINER_ACTOR_CODE_ID),
    );
    migrations.insert(
        *actorv3::POWER_ACTOR_CODE_ID,
        nil_migrator_v4(*actorv4::POWER_ACTOR_CODE_ID),
    );
    migrations.insert(
        *actorv3::SYSTEM_ACTOR_CODE_ID,
        nil_migrator_v4(*actorv4::SYSTEM_ACTOR_CODE_ID),
    );
    migrations.insert(
        *actorv3::VERIFREG_ACTOR_CODE_ID,
        nil_migrator_v4(*actorv4::VERIFREG_ACTOR_CODE_ID),
    );

    // Set of prior version code CIDs for actors to defer during iteration, for explicit migration afterwards.
    let deferred_code_ids = HashSet::<Cid>::new(); // None in this migration

    if migrations.len() + deferred_code_ids.len() != ACTORS_COUNT {
        return Err(MigrationError::IncompleteMigrationSpec(migrations.len()));
    }

    let actors_in = StateTree::new_from_root(store, &actors_root_in).unwrap();
    let mut actors_out = StateTree::new(store, StateTreeVersion::V3)
        .map_err(|e| MigrationError::StateTreeCreation(e.to_string()))?;

    actors_in
        .for_each(|addr, state| {
            if deferred_code_ids.contains(&state.code) {
                return Ok(());
            }

            let next_input = MigrationJob {
                address: addr,
                actor_state: state.clone(),
                actor_migration: migrations
                    .get(&state.code)
                    .cloned()
                    .ok_or(MigrationError::MigratorNotFound(state.code))?,
            };

            jobs.push(async move { next_input.run(store, prior_epoch) });

            Ok(())
        })
        .map_err(|e| MigrationError::MigrationJobCreate(e.to_string()))?;

    task::block_on(async {
        while let Some(job_result) = jobs.next().await {
            let result = job_result?;
            actors_out
                .set_actor(&result.address, result.actor_state)
                .map_err(|e| MigrationError::SetActorState(e.to_string()))?;
        }

        Ok(())
    })?;

    let root_cid = actors_out
        .flush()
        .map_err(|e| MigrationError::FlushFailed(e.to_string()));

    root_cid
}
