// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Common code that's shared across all migration code.
//! Each network upgrade / state migration code lives in their own module.

use std::sync::Arc;

use ahash::{HashMap, HashMapExt};
use cid::Cid;
use forest_shim::{
    address::Address,
    state_tree::{ActorState, StateTree},
    Inner,
};
use fvm_ipld_blockstore::Blockstore;
use fvm_shared::{clock::ChainEpoch, econ::TokenAmount};
use parking_lot::Mutex;

mod nv18;
pub use nv18::migration::run_migration as run_nv18_migration;

pub type Migrator<BS> = Arc<dyn ActorMigration<BS> + Send + Sync>;

/// Trait to be implemented by migration verifications.
/// The implementation should verify that the migration specification is
/// correct. This is to prevent accidental migration errors.
pub trait ActorMigrationVerifier<BS> {
    fn verify_migration(
        &self,
        store: &BS,
        migrations: &HashMap<Cid, Migrator<BS>>,
        actors_in: &StateTree<BS>,
    ) -> anyhow::Result<()>;
}

/// Type implementing the `ActorMigrationVerifier` trait.
pub type MigrationVerifier<BS> = Arc<dyn ActorMigrationVerifier<BS> + Send + Sync>;

pub type PostMigrationAction<BS> =
    Arc<dyn Fn(&BS, &mut StateTree<BS>) -> anyhow::Result<()> + Send + Sync>;

/// StateMigration handles several cases of migration:
/// - nil migrations, essentially maping one Actor to another,
/// - migrations where state upgrade is required,
/// - creating new actors that were not present in the prior network version.
pub struct StateMigration<BS> {
    migrations: HashMap<Cid, Migrator<BS>>,
    new_manifest_data: Cid,
    /// Verifies correctness of the migration specification.
    verifier: Option<MigrationVerifier<BS>>,
    /// Post migration actions. This may include new actor creation.
    post_migration_actions: Vec<PostMigrationAction<BS>>,
}

impl<BS: Blockstore + Clone + Send + Sync> StateMigration<BS> {
    pub fn new(
        new_manifest_data: Cid,
        verifier: Option<MigrationVerifier<BS>>,
        post_migration_actions: Vec<PostMigrationAction<BS>>,
    ) -> Self {
        Self {
            migrations: HashMap::new(),
            new_manifest_data,
            verifier,
            post_migration_actions,
        }
    }

    /// Inserts a new migrator into the migration specification.
    pub fn add_migrator(&mut self, prior_cid: Cid, migrator: Migrator<BS>) {
        self.migrations.insert(prior_cid, migrator);
    }

    pub fn migrate_state_tree(
        &self,
        store: BS,
        prior_epoch: ChainEpoch,
        actors_in: StateTree<BS>,
        mut actors_out: StateTree<BS>,
    ) -> anyhow::Result<Cid> {
        // Checks if the migration specification is correct
        if let Some(verifier) = &self.verifier {
            verifier.verify_migration(&store, &self.migrations, &actors_in)?;
        }

        let cpus = num_cpus::get();
        let chan_size = cpus / 2;

        log::info!(
            "Using {} CPUs for migration and channel size of {}",
            cpus,
            chan_size
        );

        let pool = rayon::ThreadPoolBuilder::new()
            .thread_name(|id| format!("state migration thread: {id}"))
            .num_threads(cpus)
            .build()?;

        let (state_tx, state_rx) = crossbeam_channel::bounded(chan_size);
        let (job_tx, job_rx) = crossbeam_channel::bounded(chan_size);

        let actors_in_counter = Arc::new(Mutex::new(0));
        let actors_out_counter = Arc::new(Mutex::new(0));

        pool.scope(|s| {
            let store_clone = store.clone();
            let actors_in_counter_clone = actors_in_counter.clone();

            s.spawn(move |_| {
                actors_in
                    .for_each(|addr, state| {
                        state_tx
                            .send((addr, state.clone()))
                            .expect("failed sending actor state through channel");
                        *actors_in_counter_clone.lock() += 1;
                        Ok(())
                    })
                    .expect("Failed iterating over actor state");
            });

            s.spawn(move |scope| {
                while let Ok((address, state)) = state_rx.recv() {
                    let job_tx = job_tx.clone();
                    let store_clone = store_clone.clone();
                    let migrator = self.migrations.get(&state.code).cloned().unwrap_or_else(|| panic!("fiasco with: {}", state.code));
                    scope.spawn(move |_| {
                        let job = MigrationJob {
                            address,
                            actor_state: state,
                            actor_migration: migrator,
                        };

                        let job_output = job.run(store_clone, prior_epoch).unwrap_or_else(|e| {
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
                let MigrationJobOutput {
                    address,
                    actor_state,
                } = job_output;
                actors_out
                    .set_actor(&address, actor_state)
                    .unwrap_or_else(|e| {
                        panic!(
                            "Failed setting new actor state at given address: {address}, Reason: {e}"
                        )
                    });
                    *actors_out_counter.lock() += 1;
            }
        });

        dbg!(*actors_in_counter.lock(), *actors_out_counter.lock());

        // execute post migration actions, e.g., create new actors
        for action in self.post_migration_actions.iter() {
            action(&store, &mut actors_out)?;
        }

        actors_out.flush()
    }
}

#[allow(dead_code)] // future migrations might need the fields.
pub struct ActorMigrationInput {
    /// Actor's address
    address: Address,
    /// Actor's balance
    balance: TokenAmount,
    /// Actor's state head CID
    head: Cid,
    /// Epoch of last state transition prior to migration
    prior_epoch: ChainEpoch,
}

pub struct MigrationOutput {
    new_code_cid: Cid,
    new_head: Cid,
}

pub trait ActorMigration<BS: Blockstore + Clone + Send + Sync> {
    fn migrate_state(
        &self,
        store: BS,
        input: ActorMigrationInput,
    ) -> anyhow::Result<MigrationOutput>;
}

struct MigrationJob<BS: Blockstore> {
    address: Address,
    actor_state: ActorState,
    actor_migration: Arc<dyn ActorMigration<BS>>,
}

impl<BS: Blockstore + Clone + Send + Sync> MigrationJob<BS> {
    fn run(&self, store: BS, prior_epoch: ChainEpoch) -> anyhow::Result<MigrationJobOutput> {
        let result = self
            .actor_migration
            .migrate_state(
                store,
                ActorMigrationInput {
                    address: self.address,
                    balance: forest_shim::econ::TokenAmount::from(&self.actor_state.balance).into(),
                    head: self.actor_state.state,
                    prior_epoch,
                    // TODO: Lotus adds some kind of a cache, may need to investigate it
                },
            )
            .map_err(|e| {
                anyhow::anyhow!(
                    "state migration failed for {} actor, addr {}:{}",
                    self.actor_state.code,
                    self.address,
                    e
                )
            })?;

        let migration_job_result = MigrationJobOutput {
            address: self.address,
            actor_state: <ActorState as Inner>::FVM::new(
                result.new_code_cid,
                result.new_head,
                self.actor_state.balance.clone(),
                self.actor_state.sequence,
                None,
            )
            .into(),
        };

        Ok(migration_job_result)
    }
}

#[derive(Debug)]
struct MigrationJobOutput {
    address: Address,
    actor_state: ActorState,
}

#[allow(dead_code)]
fn nil_migrator<BS: Blockstore + Clone + Send + Sync>(
    cid: Cid,
) -> Arc<dyn ActorMigration<BS> + Send + Sync> {
    Arc::new(NilMigrator(cid))
}

/// Migrator which preserves the head CID and provides a fixed result code CID.
pub(crate) struct NilMigrator(Cid);

impl<BS: Blockstore + Clone + Send + Sync> ActorMigration<BS> for NilMigrator {
    fn migrate_state(
        &self,
        _store: BS,
        input: ActorMigrationInput,
    ) -> anyhow::Result<MigrationOutput> {
        Ok(MigrationOutput {
            new_code_cid: self.0,
            new_head: input.head,
        })
    }
}
