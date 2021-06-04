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
use async_std::sync::Arc;
use cid::Cid;
use clock::ChainEpoch;
use fil_types::StateTreeVersion;
use ipld_blockstore::BlockStore;
use miner::miner_migrator_v4;
use state_tree::StateTree;
use std::collections::{HashMap, HashSet};

type Migrator<BS> = Arc<dyn ActorMigration<BS> + Send + Sync>;

const ACTORS_COUNT: usize = 11;

// Try to pass an Arc<BS> here.
pub fn migrate_state_tree<BS: BlockStore + Send + Sync>(
    store: Arc<BS>,
    actors_root_in: Cid,
    prior_epoch: ChainEpoch,
) -> MigrationResult<Cid> {
    // Maps prior version code CIDs to migration functions.
    let mut migrations: HashMap<Cid, Migrator<BS>> = HashMap::with_capacity(ACTORS_COUNT);
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

    let actors_in = StateTree::new_from_root(&*store, &actors_root_in).map_err(|e| MigrationError::StateTreeCreation(e.to_string()))?;
    let mut actors_out = StateTree::new(&*store, StateTreeVersion::V3)
        .map_err(|e| MigrationError::StateTreeCreation(e.to_string()))?;

    let cpus = num_cpus::get();
    let chan_size = 2;
    log::info!(
        "Using {} CPUs for migration and channel size of {}",
        cpus,
        chan_size
    );

    let pool = rayon::ThreadPoolBuilder::new()
        .thread_name(|id| format!("nv12 migration thread: {}", id))
        .num_threads(cpus)
        .build()
        .map_err(|e| MigrationError::ThreadPoolCreation(e))?;

    let (state_tx, state_rx) = crossbeam_channel::bounded(chan_size);
    let (job_tx, job_rx) = crossbeam_channel::bounded(chan_size);

    pool.scope(|s| {
        let store_clone = store.clone();

        s.spawn(move |_| {
            actors_in
                .for_each(|addr, state| {
                    state_tx
                        .send((addr, state.clone()))
                        .expect("failed sending actor state through channel");
                    Ok(())
                })
                .expect("Failed iterating over actor state");
        });

        s.spawn(move |scope| {
            while let Ok((addr, state)) = state_rx.recv() {
                let job_tx = job_tx.clone();
                let store_clone = store_clone.clone();
                let migrator = migrations.get(&state.code).cloned().unwrap();
                scope.spawn(move |_| {
                    let job = MigrationJob {
                        address: addr.clone(),
                        actor_state: state,
                        actor_migration: migrator,
                    };

                    let job_output = job
                        .run(store_clone, prior_epoch)
                        .expect(&format!("failed executing job for address: {}", addr));

                    job_tx
                        .send(job_output)
                        .expect(&format!("failed sending job output for address: {}", addr));
                });
            }
            drop(job_tx);
        });

        while let Ok(job_output) = job_rx.recv() {
            actors_out
                .set_actor(&job_output.address, job_output.actor_state)
                .expect(&format!(
                    "Failed setting new actor state at given address: {}",
                    job_output.address
                ));
        }
    });

    let root_cid = actors_out
        .flush()
        .map_err(|e| MigrationError::FlushFailed(e.to_string()));

    root_cid
}
