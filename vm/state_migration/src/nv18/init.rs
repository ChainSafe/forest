// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! This module contains the migration logic for the `NV18` upgrade for the Init
//! actor.

use std::sync::Arc;

use cid::Cid;
use fil_actor_init_state::{v10::State as InitStateNew, v9::State as InitStateOld};
use forest_utils::db::CborStoreExt;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::CborStore;

use crate::common::{
    ActorMigration, ActorMigrationInput, ActorMigrationOutput, TypeMigration, TypeMigrator,
};

pub struct InitMigrator(Cid);

pub(crate) fn init_migrator<BS: Blockstore + Clone + Send + Sync>(
    cid: Cid,
) -> Arc<dyn ActorMigration<BS> + Send + Sync> {
    Arc::new(InitMigrator(cid))
}

impl<BS: Blockstore + Clone + Send + Sync> ActorMigration<BS> for InitMigrator {
    fn migrate_state(
        &self,
        store: BS,
        input: ActorMigrationInput,
    ) -> anyhow::Result<ActorMigrationOutput> {
        let in_state: InitStateOld = store
            .get_cbor(&input.head)?
            .ok_or_else(|| anyhow::anyhow!("Init actor: could not read v9 state"))?;

        let out_state: InitStateNew = TypeMigrator::migrate_type(in_state, &store)?;

        let new_head = store.put_cbor_default(&out_state)?;

        Ok(ActorMigrationOutput {
            new_code_cid: self.0,
            new_head,
        })
    }
}
