// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use crate::cid_collections::CidHashMap;
use crate::shim::{clock::ChainEpoch, state_tree::StateTree};
use crate::state_migration::common::MigrationCache;
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use parking_lot::Mutex;

use super::PostMigrationCheckArc;
use super::{Migrator, PostMigratorArc, verifier::MigrationVerifier};
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
    /// Post migration checks. This is used to verify the correctness of the migration.
    post_migration_checks: Vec<PostMigrationCheckArc<BS>>,
}

impl<BS: Blockstore> StateMigration<BS> {
    pub(in crate::state_migration) fn new(verifier: Option<MigrationVerifier<BS>>) -> Self {
        Self {
            migrations: CidHashMap::new(),
            verifier,
            post_migrators: Default::default(),
            post_migration_checks: Default::default(),
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

    /// Inserts a new post migration check into the post migration checks specification.
    pub(in crate::state_migration) fn add_post_migration_check(
        &mut self,
        post_migration_check: PostMigrationCheckArc<BS>,
    ) {
        self.post_migration_checks.push(post_migration_check);
    }
}

impl<BS: Blockstore + Send + Sync> StateMigration<BS> {
    pub(in crate::state_migration) fn migrate_state_tree(
        &self,
        store: &BS,
        prior_epoch: ChainEpoch,
        actors_in: StateTree<BS>,
        mut actors_out: StateTree<BS>,
    ) -> anyhow::Result<Cid> {
        // Checks if the migration specification is correct
        if let Some(verifier) = &self.verifier {
            verifier.verify_migration(store, &self.migrations, &actors_in)?;
        }

        let cache = MigrationCache::new(NonZeroUsize::new(10_000).expect("infallible"));
        let num_threads = std::env::var("FOREST_STATE_MIGRATION_THREADS")
            .ok()
            .and_then(|s| s.parse().ok())
            // Don't use all CPU, otherwise the migration will starve the rest of the system.
            .unwrap_or_else(|| num_cpus::get() / 2)
            // At least 3 are required to not deadlock the migration.
            .max(3);

        let pool = rayon::ThreadPoolBuilder::new()
            .thread_name(|id| format!("state migration thread: {id}"))
            .num_threads(num_threads)
            .build()?;

        let (state_tx, state_rx) = flume::bounded(30);
        let (job_tx, job_rx) = flume::bounded(30);

        let job_counter = AtomicU64::new(0);
        let cache_clone = cache.clone();

        let actors_in = Arc::new(Mutex::new(actors_in));
        let actors_in_clone = actors_in.clone();
        pool.scope(|s| {
            s.spawn(move |_| {
                actors_in.lock()
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
                    let migrator = self.migrations.get(&state.code).cloned().unwrap_or_else(|| panic!("migration failed with state code: {}", state.code));

                    // Deferred migrations should be done at a later time.
                    if migrator.is_deferred() {
                        continue;
                    }
                    let cache_clone = cache_clone.clone();
                    scope.spawn(move |_| {
                        let job = MigrationJob {
                            address,
                            actor_state: state,
                            actor_migration: migrator,
                        };

                        let job_output = job.run(store, prior_epoch, cache_clone).unwrap_or_else(|e| {
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
                    job_counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let job_counter = job_counter.load(std::sync::atomic::Ordering::Relaxed);
                    if job_counter % 100_000 == 0 {
                        tracing::info!("Processed {job_counter} actors", job_counter = job_counter);
                    }
                }
            }
        });

        // This is okay to execute even if there are no deferred migrations, as the iteration is
        // very cheap; ~200ms on mainnet. The alternative is to collect the deferred migrations
        // into a separate collection, which would increase the memory footprint of the migration.
        tracing::info!("Processing deferred migrations");
        let mut job_counter = 0;
        actors_in_clone.lock().for_each(|address, state| {
            job_counter += 1;
            let migrator = self
                .migrations
                .get(&state.code)
                .cloned()
                .unwrap_or_else(|| panic!("migration failed with state code: {}", state.code));

            if !migrator.is_deferred() {
                return Ok(());
            }

            let job = MigrationJob {
                address,
                actor_state: state.clone(),
                actor_migration: migrator,
            };
            let job_output = job.run(store, prior_epoch, cache.clone())?;
            if let Some(MigrationJobOutput {
                address,
                actor_state,
            }) = job_output
            {
                actors_out
                    .set_actor(&address, actor_state)
                    .unwrap_or_else(|e| {
                        panic!(
                            "Failed setting new actor state at given address: {address}, Reason: {e}"
                        )
                    });
            }

            Ok(())
        })?;
        tracing::info!("Processed {job_counter} deferred migrations");

        // execute post migration actions, e.g., create new actors
        for post_migrator in self.post_migrators.iter() {
            post_migrator.post_migrate_state(store, &mut actors_out)?;
        }

        // execute post migration checks
        for post_migration_check in self.post_migration_checks.iter() {
            post_migration_check.post_migrate_check(store, &actors_out)?;
        }

        actors_out.flush()
    }
}
