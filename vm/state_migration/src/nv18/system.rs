// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

use cid::{multihash::Code::Blake2b256, Cid};
use fil_actor_system_v10::State as StateV10;
use forest_utils::db::BlockstoreExt;
use fvm_ipld_blockstore::Blockstore;

use super::calibnet;
use crate::{ActorMigration, ActorMigrationInput, MigrationOutput};

pub struct SystemMigrator(Cid);

pub fn system_migrator<BS: Blockstore + Clone + Send + Sync>(
    cid: Cid,
) -> Arc<dyn ActorMigration<BS> + Send + Sync> {
    Arc::new(SystemMigrator(cid))
}

impl<BS: Blockstore + Clone + Send + Sync> ActorMigration<BS> for SystemMigrator {
    fn migrate_state(
        &self,
        store: BS,
        _input: ActorMigrationInput,
    ) -> anyhow::Result<MigrationOutput> {
        // TODO get it in a sane way from manifest
        let state = StateV10 {
            builtin_actors: Cid::try_from(
                "bafy2bzacec4ayvs43rn4j3ve3usnk4f2mor6wbxqkahjyokvd6ti2rclq35du",
            )?,
        };
        let new_head = store.put_obj(&state, Blake2b256)?;

        Ok(MigrationOutput {
            new_code_cid: self.0,
            new_head,
        })
    }
}
