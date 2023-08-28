// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

use cid::Cid;
use fvm_ipld_blockstore::Blockstore;

use super::{ActorMigration, ActorMigrationInput, ActorMigrationOutput};

/// Migrator which preserves the head CID and provides a fixed result code CID.
/// This is used to migrate actors which do not require any state migration.
pub(in crate::state_migration) struct NilMigrator(Cid);

impl<BS: Blockstore> ActorMigration<BS> for NilMigrator {
    fn migrate_state(
        &self,
        _store: &BS,
        input: ActorMigrationInput,
    ) -> anyhow::Result<Option<ActorMigrationOutput>> {
        Ok(Some(ActorMigrationOutput {
            new_code_cid: self.0,
            new_head: input.head,
        }))
    }
}

/// Creates a new migrator which preserves the head CID and provides a fixed
/// result code CID.
pub(in crate::state_migration) fn nil_migrator<BS: Blockstore>(
    cid: Cid,
) -> Arc<dyn ActorMigration<BS> + Send + Sync> {
    Arc::new(NilMigrator(cid))
}

/// A migrator that does nothing but delegates the explicit migration logic to post migrator(s)
pub(in crate::state_migration) struct DeferredMigrator;

impl<BS: Blockstore> ActorMigration<BS> for DeferredMigrator {
    fn migrate_state(
        &self,
        _store: &BS,
        _input: ActorMigrationInput,
    ) -> anyhow::Result<Option<ActorMigrationOutput>> {
        Ok(None)
    }
}
