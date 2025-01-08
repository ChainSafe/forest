// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::shim::{
    address::Address,
    machine::{BuiltinActor, BuiltinActorManifest},
    state_tree::{ActorState, StateTree},
};
use crate::utils::db::CborStoreExt as _;
use fvm_ipld_blockstore::Blockstore;

use crate::state_migration::common::PostMigrator;

use super::SystemStateNew;

pub struct EamPostMigrator;

impl<BS: Blockstore> PostMigrator<BS> for EamPostMigrator {
    /// Creates the Ethereum Account Manager actor in the state tree.
    fn post_migrate_state(&self, store: &BS, actors_out: &mut StateTree<BS>) -> anyhow::Result<()> {
        let sys_actor = actors_out.get_required_actor(&Address::SYSTEM_ACTOR)?;
        let sys_state: SystemStateNew = store.get_cbor_required(&sys_actor.state)?;

        let manifest = BuiltinActorManifest::load_v1_actor_list(store, &sys_state.builtin_actors)?;

        let eam_actor = ActorState::new_empty(manifest.get(BuiltinActor::EAM)?, None);
        actors_out.set_actor(&Address::ETHEREUM_ACCOUNT_MANAGER_ACTOR, eam_actor)?;
        Ok(())
    }
}
