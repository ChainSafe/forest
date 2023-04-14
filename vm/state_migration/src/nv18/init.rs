// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! This module contains the migration logic for the `NV18` upgrade for the Init
//! actor.

use std::sync::Arc;

use cid::{multihash::Code::Blake2b256, Cid};
use fil_actor_init_v10::State as StateV10;
use fil_actor_init_v9::State as StateV9;
use fil_actors_runtime_v10::{make_map_with_root, Map};
use forest_shim::{
    address::{Address, PAYLOAD_HASH_LEN},
    state_tree::ActorID,
};
use forest_utils::db::BlockstoreExt;
use fvm_ipld_blockstore::Blockstore;

use crate::common::{ActorMigration, ActorMigrationInput, ActorMigrationOutput};

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
        let in_state: StateV9 = store
            .get_obj(&input.head)?
            .ok_or_else(|| anyhow::anyhow!("Init actor: could not read v9 state"))?;

        let mut in_addr_map: Map<_, ActorID> = make_map_with_root(&in_state.address_map, &store)
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        let actor_id = in_state.next_id;
        let eth_zero_addr = Address::new_delegated(
            Address::ETHEREUM_ACCOUNT_MANAGER_ACTOR.id()?,
            &[0; PAYLOAD_HASH_LEN],
        )?;
        in_addr_map.set(eth_zero_addr.to_bytes().into(), actor_id)?;

        let out_state = StateV10 {
            address_map: in_addr_map.flush()?,
            next_id: in_state.next_id + 1,
            network_name: in_state.network_name,
        };

        let new_head = store.put_obj(&out_state, Blake2b256)?;

        Ok(ActorMigrationOutput {
            new_code_cid: self.0,
            new_head,
        })
    }
}
