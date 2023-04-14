// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! This module contains the migration logic for the `NV18` upgrade for the
//! System actor.
use std::sync::Arc;

use cid::{multihash::Code::Blake2b256, Cid};
use fil_actor_system_v10::State as StateV10;
use forest_utils::db::BlockstoreExt;
use fvm_ipld_blockstore::Blockstore;

use crate::common::{ActorMigration, ActorMigrationInput, ActorMigrationOutput};

pub(crate) struct SystemMigrator {
    new_builtin_actors_cid: Cid,
    new_code_cid: Cid,
}

pub(crate) fn system_migrator<BS: Blockstore + Clone + Send + Sync>(
    new_builtin_actors_cid: Cid,
    new_code_cid: Cid,
) -> Arc<dyn ActorMigration<BS> + Send + Sync> {
    Arc::new(SystemMigrator {
        new_builtin_actors_cid,
        new_code_cid,
    })
}

impl<BS: Blockstore + Clone + Send + Sync> ActorMigration<BS> for SystemMigrator {
    fn migrate_state(
        &self,
        store: BS,
        _input: ActorMigrationInput,
    ) -> anyhow::Result<ActorMigrationOutput> {
        let state = StateV10 {
            builtin_actors: self.new_builtin_actors_cid,
        };
        let new_head = store.put_obj(&state, Blake2b256)?;

        Ok(ActorMigrationOutput {
            new_code_cid: self.new_code_cid,
            new_head,
        })
    }
}
