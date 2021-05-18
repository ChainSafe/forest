// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Common code that's shared across all migration code.
//! Each network upgrade / state migration code lives in their own module.

use address::Address;
use cid::Cid;
use clock::ChainEpoch;
use ipld_blockstore::BlockStore;
use std::error::Error as StdError;
use std::rc::Rc;
use vm::{ActorState, TokenAmount};

pub mod nv12;

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
    #[error("Migration failed")]
    Other,
}

pub(crate) struct ActorMigrationInput {
    /// Actor's address
    address: Address,
    /// Actor's balance
    balance: TokenAmount,
    /// Actor's state head CID
    head: Cid,
    /// Epoch of last state transition prior to migration
    prior_epoch: ChainEpoch,
}

pub(crate) struct MigrationOutput {
    new_code_cid: Cid,
    new_head: Cid,
}

pub(crate) trait ActorMigration<'db, BS: BlockStore> {
    fn migrate_state(
        &self,
        store: &'db BS,
        input: ActorMigrationInput,
    ) -> MigrationResult<MigrationOutput>;
}

struct MigrationJob<'db, BS: BlockStore> {
    address: Address,
    actor_state: ActorState,
    actor_migration: Rc<dyn ActorMigration<'db, BS>>,
}

impl<'db, BS: BlockStore> MigrationJob<'db, BS> {
    fn run(&self, store: &'db BS, prior_epoch: ChainEpoch) -> MigrationResult<MigrationJobOutput> {
        let result = self
            .actor_migration
            .migrate_state(
                store,
                ActorMigrationInput {
                    address: self.address,
                    balance: self.actor_state.balance.clone(),
                    head: self.actor_state.state,
                    prior_epoch: prior_epoch,
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

fn nil_migrator_v4<'db, BS: BlockStore>(cid: Cid) -> Rc<dyn ActorMigration<'db, BS>> {
    Rc::new(NilMigrator(cid))
}

// Migrator which preserves the head CID and provides a fixed result code CID.
pub(crate) struct NilMigrator(Cid);

impl<'db, BS: BlockStore> ActorMigration<'db, BS> for NilMigrator {
    fn migrate_state(
        &self,
        _store: &'db BS,
        input: ActorMigrationInput,
    ) -> MigrationResult<MigrationOutput> {
        Ok(MigrationOutput {
            new_code_cid: self.0,
            new_head: input.head,
        })
    }
}
