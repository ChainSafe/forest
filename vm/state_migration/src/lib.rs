// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Common code that's shared across all migration code.
//! Each network upgrade / state migration code lives in their own module.

use address::Address;
use cid::Cid;
use clock::ChainEpoch;
use ipld_blockstore::BlockStore;
use state_tree::StateTree;
use vm::{ActorState, TokenAmount};

use async_std::sync::Arc;
use rayon::ThreadPoolBuildError;
use std::collections::{HashMap, HashSet};

pub mod nv12;

pub const ACTORS_COUNT: usize = 11;

pub type Migrator<BS> = Arc<dyn ActorMigration<BS> + Send + Sync>;
pub type MigrationResult<T> = Result<T, MigrationError>;

#[derive(thiserror::Error, Debug)]
pub enum MigrationError {
    // FIXME: use underlying concrete types when possible.
    #[error("Failed creating job for state migration: {0}")]
    MigrationJobCreate(String),
    #[error("Failed running job for state migration: {0}")]
    MigrationJobRun(String),
    #[error("Flush failed post migration: {0}")]
    FlushFailed(String),
    #[error("Failed writing to blockstore: {0}")]
    BlockStoreWrite(String),
    #[error("Failed reading from blockstore: {0}")]
    BlockStoreRead(String),
    #[error("Migrator not found for cid: {0}")]
    MigratorNotFound(Cid),
    #[error("Failed updating new actor state: {0}")]
    SetActorState(String),
    #[error("State tree creation failed")]
    StateTreeCreation(String),
    #[error("Incomplete migration specification with {0} code CIDs")]
    IncompleteMigrationSpec(usize),
    #[error("Thread pool creation failed: {0}")]
    ThreadPoolCreation(ThreadPoolBuildError),
    #[error("Migration failed")]
    Other,
}

pub struct StateMigration<BS> {
    migrations: HashMap<Cid, Migrator<BS>>,
    deferred_code_ids: HashSet<Cid>,
}

impl<BS: BlockStore + Send + Sync> StateMigration<BS> {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            migrations: HashMap::new(),
            deferred_code_ids: HashSet::new(),
        }
    }

    pub fn add_migrator(&mut self, prior_cid: Cid, migrator: Migrator<BS>) {
        self.migrations.insert(prior_cid, migrator);
    }

    pub fn migrate_state_tree(
        &mut self,
        store: Arc<BS>,
        prior_epoch: ChainEpoch,
        actors_in: StateTree<BS>,
        mut actors_out: StateTree<BS>,
    ) -> MigrationResult<Cid> {
        if self.migrations.len() + self.deferred_code_ids.len() != ACTORS_COUNT {
            return Err(MigrationError::IncompleteMigrationSpec(
                self.migrations.len(),
            ));
        }

        let cpus = num_cpus::get();
        let chan_size = cpus / 2;

        log::info!(
            "Using {} CPUs for migration and channel size of {}",
            cpus,
            chan_size
        );

        let pool = rayon::ThreadPoolBuilder::new()
            .thread_name(|id| format!("nv12 migration thread: {}", id))
            .num_threads(cpus)
            .build()
            .map_err(MigrationError::ThreadPoolCreation)?;

        let (state_tx, state_rx) = crossbeam_channel::bounded(chan_size);
        let (job_tx, job_rx) = crossbeam_channel::bounded(chan_size);

        pool.scope(|s| {
            let store_clone = store.clone();

            s.spawn(move |_| {
                // TODO: actors_in (StateTree) could use an iterator here.
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
                    let store_clone = store_clone.clone();
                    let migrator = self.migrations.get(&state.code).cloned().unwrap();
                    scope.spawn(move |_| {
                        let job = MigrationJob {
                            address,
                            actor_state: state,
                            actor_migration: migrator,
                        };

                        let job_output = job.run(store_clone, prior_epoch).unwrap_or_else(|e| {
                            panic!(
                                "failed executing job for address: {}, Reason: {}",
                                address, e
                            )
                        });

                        job_tx.send(job_output).unwrap_or_else(|_| {
                            panic!("failed sending job output for address: {}", address)
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
                            "Failed setting new actor state at given address: {}, Reason: {}",
                            address, e
                        )
                    });
            }
        });

        let root_cid = actors_out
            .flush()
            .map_err(|e| MigrationError::FlushFailed(e.to_string()));

        root_cid
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

pub trait ActorMigration<BS: BlockStore + Send + Sync> {
    fn migrate_state(
        &self,
        store: Arc<BS>,
        input: ActorMigrationInput,
    ) -> MigrationResult<MigrationOutput>;
}

struct MigrationJob<BS: BlockStore> {
    address: Address,
    actor_state: ActorState,
    actor_migration: Arc<dyn ActorMigration<BS>>,
}

impl<BS: BlockStore + Send + Sync> MigrationJob<BS> {
    fn run(&self, store: Arc<BS>, prior_epoch: ChainEpoch) -> MigrationResult<MigrationJobOutput> {
        let result = self
            .actor_migration
            .migrate_state(
                store,
                ActorMigrationInput {
                    address: self.address,
                    balance: self.actor_state.balance.clone(),
                    head: self.actor_state.state,
                    prior_epoch,
                },
            )
            .map_err(|e| {
                MigrationError::MigrationJobRun(format!(
                    "state migration failed for {} actor, addr {}:{}",
                    self.actor_state.code,
                    self.address,
                    e.to_string()
                ))
            })?;

        let migration_job_result = MigrationJobOutput {
            address: self.address,
            actor_state: ActorState::new(
                result.new_code_cid,
                result.new_head,
                self.actor_state.balance.clone(),
                self.actor_state.sequence,
            ),
        };

        Ok(migration_job_result)
    }
}

#[derive(Debug)]
struct MigrationJobOutput {
    address: Address,
    actor_state: ActorState,
}

fn nil_migrator<BS: BlockStore + Send + Sync>(
    cid: Cid,
) -> Arc<dyn ActorMigration<BS> + Send + Sync> {
    Arc::new(NilMigrator(cid))
}

/// Migrator which preserves the head CID and provides a fixed result code CID.
pub(crate) struct NilMigrator(Cid);

impl<BS: BlockStore + Send + Sync> ActorMigration<BS> for NilMigrator {
    fn migrate_state(
        &self,
        _store: Arc<BS>,
        input: ActorMigrationInput,
    ) -> MigrationResult<MigrationOutput> {
        Ok(MigrationOutput {
            new_code_cid: self.0,
            new_head: input.head,
        })
    }
}
