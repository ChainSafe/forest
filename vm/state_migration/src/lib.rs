// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Common code that's shared across all migration code.
//! Each network upgrade / state migration code lives in their own module.

use std::sync::Arc;

use ahash::{HashMap, HashMapExt, HashSet, HashSetExt};
use cid::Cid;
use forest_shim::{
    address::Address,
    state_tree::{ActorState, StateTree},
    Inner,
};
use fvm_ipld_blockstore::Blockstore;
use fvm_shared::{clock::ChainEpoch, econ::TokenAmount};
use rayon::ThreadPoolBuildError;

// pub mod nv12;

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

impl<BS: Blockstore + Clone + Send + Sync> StateMigration<BS> {
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
        todo!()
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

pub trait ActorMigration<BS: Blockstore + Send + Sync> {
    fn migrate_state(
        &self,
        store: Arc<BS>,
        input: ActorMigrationInput,
    ) -> MigrationResult<MigrationOutput>;
}

struct MigrationJob<BS: Blockstore> {
    address: Address,
    actor_state: ActorState,
    actor_migration: Arc<dyn ActorMigration<BS>>,
}

impl<BS: Blockstore + Send + Sync> MigrationJob<BS> {
    fn run(&self, store: Arc<BS>, prior_epoch: ChainEpoch) -> MigrationResult<MigrationJobOutput> {
        let result = self
            .actor_migration
            .migrate_state(
                store,
                ActorMigrationInput {
                    address: self.address,
                    balance: forest_shim::econ::TokenAmount::from(&self.actor_state.balance).into(),
                    head: self.actor_state.state,
                    prior_epoch,
                },
            )
            .map_err(|e| {
                MigrationError::MigrationJobRun(format!(
                    "state migration failed for {} actor, addr {}:{}",
                    self.actor_state.code, self.address, e
                ))
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
fn nil_migrator<BS: Blockstore + Send + Sync>(
    cid: Cid,
) -> Arc<dyn ActorMigration<BS> + Send + Sync> {
    Arc::new(NilMigrator(cid))
}

/// Migrator which preserves the head CID and provides a fixed result code CID.
pub(crate) struct NilMigrator(Cid);

impl<BS: Blockstore + Send + Sync> ActorMigration<BS> for NilMigrator {
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
