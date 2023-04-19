// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! This module contains the migration logic for the `NV18` upgrade for the Init
//! actor.

use std::sync::Arc;

use cid::{multihash::Code::Blake2b256, Cid};
use fil_actor_miner_v10::{MinerInfo, State as StateV10};
use fil_actor_miner_v11::State as StateV11;
use fil_actors_runtime_v11::{make_map_with_root, Map};
use forest_shim::{
    address::{Address, PAYLOAD_HASH_LEN},
    state_tree::ActorID,
};
use forest_utils::db::BlockstoreExt;
use fvm_ipld_blockstore::Blockstore;

use crate::common::{ActorMigration, ActorMigrationInput, ActorMigrationOutput};

pub struct MinerMigrator(Cid);

pub(crate) fn miner_migrator<BS: Blockstore + Clone + Send + Sync>(
    cid: Cid,
) -> Arc<dyn ActorMigration<BS> + Send + Sync> {
    Arc::new(MinerMigrator(cid))
}

impl<BS: Blockstore + Clone + Send + Sync> ActorMigration<BS> for MinerMigrator {
    fn migrate_state(
        &self,
        store: BS,
        input: ActorMigrationInput,
    ) -> anyhow::Result<ActorMigrationOutput> {
        let in_state: StateV10 = store
            .get_obj(&input.head)?
            .ok_or_else(|| anyhow::anyhow!("Miner actor: could not read v10 state"))?;

        let in_info: MinerInfo = store
            .get_obj(&input.head)?
            .ok_or_else(|| anyhow::anyhow!("Miner info: could not read v10 state"))?;

        let out_proof_type = convert_window_post_proof_v1p1_to_v1(in_info.window_post_proof_type);

        let out_info = MinerInfo {
            // TODO: check if we need to pass pending worker key
            window_post_proof_type: out_proof_type,
            ..in_info
        };

        let out_info_cid = store.put_obj(&out_info, Blake2b256)?;

        let out_state = StateV11 {
            info: out_info_cid,
            ..in_state
        };

        let new_head = store.put_obj(&out_state, Blake2b256)?;

        Ok(ActorMigrationOutput {
            new_code_cid: self.0,
            new_head,
        })
    }
}
