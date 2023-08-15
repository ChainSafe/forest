// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

use crate::ipld::CidHashMap;
use crate::shim::{clock::ChainEpoch, state_tree::StateTree};
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;

use super::{verifier::MigrationVerifier, Migrator, PostMigratorArc};
use crate::state_migration::common::migration_job::{MigrationJob, MigrationJobOutput};

/// Handles several cases of migration:
/// - nil migrations, essentially mapping one Actor to another,
/// - migrations where state upgrade is required,
/// - creating new actors that were not present in the prior network version.
pub(in crate::state_migration) struct StateMigration<BS> {
    migrations: CidHashMap<Migrator<BS>>,
    /// Verifies correctness of the migration specification.
    verifier: Option<MigrationVerifier<BS>>,
    /// Post migrator(s). This may include new actor creation.
    post_migrators: Vec<PostMigratorArc<BS>>,
}

impl<BS: Blockstore> StateMigration<BS> {
    pub(in crate::state_migration) fn new(verifier: Option<MigrationVerifier<BS>>) -> Self {
        Self {
            migrations: CidHashMap::new(),
            verifier,
            post_migrators: Default::default(),
        }
    }

    /// Inserts a new migrator into the migration specification.
    pub(in crate::state_migration) fn add_migrator(
        &mut self,
        prior_cid: Cid,
        migrator: Migrator<BS>,
    ) {
        self.migrations.insert(prior_cid, migrator);
    }

    /// Inserts a new post migrator into the post migration specification.
    pub(in crate::state_migration) fn add_post_migrator(
        &mut self,
        post_migrator: PostMigratorArc<BS>,
    ) {
        self.post_migrators.push(post_migrator);
    }
}

impl<BS: Blockstore + Send + Sync> StateMigration<BS> {
    pub(in crate::state_migration) fn migrate_state_tree(
        &self,
        store: &Arc<BS>,
        prior_epoch: ChainEpoch,
        actors_in: StateTree<BS>,
        mut actors_out: StateTree<BS>,
    ) -> anyhow::Result<Cid> {
        // Checks if the migration specification is correct
        if let Some(verifier) = &self.verifier {
            verifier.verify_migration(store, &self.migrations, &actors_in)?;
        }

        // we need at least 3 threads for the migration to work
        let threads = num_cpus::get().max(3);
        let chan_size = threads / 2;

        tracing::info!("Using {threads} threads for migration and channel size of {chan_size}",);

        let pool = rayon::ThreadPoolBuilder::new()
            .thread_name(|id| format!("state migration thread: {id}"))
            .num_threads(threads)
            .build()?;

        let (state_tx, state_rx) = crossbeam_channel::bounded(chan_size);
        let (job_tx, job_rx) = crossbeam_channel::bounded(chan_size);

        pool.scope(|s| {
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
                while let Ok((address, state)) = state_rx.recv() {
                    let job_tx = job_tx.clone();
                    let migrator = self.migrations.get(state.code).cloned().unwrap_or_else(|| panic!("migration failed with state code: {}", state.code));
                    scope.spawn(move |_| {
                        let job = MigrationJob {
                            address,
                            actor_state: state,
                            actor_migration: migrator,
                        };

                        let job_output = job.run(store, prior_epoch).unwrap_or_else(|e| {
                            panic!(
                                "failed executing job for address: {address}, Reason: {e}"
                            )
                        });

                        job_tx.send(job_output).unwrap_or_else(|_| {
                            panic!("failed sending job output for address: {address}")
                        });
                    });
                }
                drop(job_tx);
            });

            while let Ok(job_output) = job_rx.recv() {
                if let Some(MigrationJobOutput {
                    address,
                    actor_state,
                }) = job_output {
                    actors_out
                        .set_actor(&address, actor_state)
                        .unwrap_or_else(|e| {
                            panic!(
                                "Failed setting new actor state at given address: {address}, Reason: {e}"
                            )
                        });
                }
            }
        });

        // execute post migration actions, e.g., create new actors
        for post_migrator in self.post_migrators.iter() {
            post_migrator.post_migrate_state(store, &mut actors_out)?;
        }

        actors_out.flush()
    }
}
