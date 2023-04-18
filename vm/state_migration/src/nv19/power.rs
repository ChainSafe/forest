// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! This module contains the migration logic for the `NV18` upgrade for the Init
//! actor.

use std::sync::Arc;

use cid::{multihash::Code::Blake2b256, Cid};
use fil_actor_power_v11::State as StateV11;
use fil_actor_power_v10::State as StateV10;
use fil_actors_runtime_v11::{make_map_with_root, Map};
use forest_shim::{
    address::{Address, PAYLOAD_HASH_LEN},
    state_tree::ActorID,
};
use forest_utils::db::BlockstoreExt;
use fvm_ipld_blockstore::Blockstore;

use crate::common::{ActorMigration, ActorMigrationInput, ActorMigrationOutput};

pub struct PowerMigrator(Cid);

pub(crate) fn power_migrator<BS: Blockstore + Clone + Send + Sync>(
    cid: Cid,
) -> Arc<dyn ActorMigration<BS> + Send + Sync> {
    Arc::new(PowerMigrator(cid))
}

impl<BS: Blockstore + Clone + Send + Sync> ActorMigration<BS> for PowerMigrator {
    fn migrate_state(
        &self,
        store: BS,
        input: ActorMigrationInput,
    ) -> anyhow::Result<ActorMigrationOutput> {
        let in_state: StateV10 = store
            .get_obj(&input.head)?
            .ok_or_else(|| anyhow::anyhow!("Power actor: could not read v10 state"))?;

        //

        let out_state = StateV11 {
            // TODO
        };

        let new_head = store.put_obj(&out_state, Blake2b256)?;

        Ok(ActorMigrationOutput {
            new_code_cid: self.0,
            new_head,
        })
    }
}
