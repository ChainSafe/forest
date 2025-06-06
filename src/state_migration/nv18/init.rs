// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! This module contains the migration logic for the `NV18` upgrade for the Init
//! actor.

use std::sync::Arc;

use crate::state_migration::common::{
    ActorMigration, ActorMigrationInput, ActorMigrationOutput, TypeMigration, TypeMigrator,
};
use crate::utils::db::CborStoreExt;
use cid::Cid;
use fil_actor_init_state::{v9::State as InitStateOld, v10::State as InitStateNew};
use fvm_ipld_blockstore::Blockstore;

pub struct InitMigrator(Cid);

pub(in crate::state_migration) fn init_migrator<BS: Blockstore>(
    cid: Cid,
) -> Arc<dyn ActorMigration<BS> + Send + Sync> {
    Arc::new(InitMigrator(cid))
}

impl<BS: Blockstore> ActorMigration<BS> for InitMigrator {
    fn migrate_state(
        &self,
        store: &BS,
        input: ActorMigrationInput,
    ) -> anyhow::Result<Option<ActorMigrationOutput>> {
        let in_state: InitStateOld = store.get_cbor_required(&input.head)?;
        let out_state: InitStateNew = TypeMigrator::migrate_type(in_state, &store)?;
        let new_head = store.put_cbor_default(&out_state)?;
        Ok(Some(ActorMigrationOutput {
            new_code_cid: self.0,
            new_head,
        }))
    }
}
