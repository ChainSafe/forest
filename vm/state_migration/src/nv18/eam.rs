// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_shim::{
    address::Address,
    machine::Manifest,
    state_tree::{ActorState, StateTree},
};
use forest_utils::db::BlockstoreExt;
use fvm_ipld_blockstore::Blockstore;

use super::SystemStateNew;

/// Creates the Ethereum Account Manager actor in the state tree.
pub fn create_eam_actor<BS: Blockstore + Clone + Send + Sync>(
    store: &BS,
    actors_out: &mut StateTree<BS>,
) -> anyhow::Result<()> {
    let sys_actor = actors_out
        .get_actor(&Address::SYSTEM_ACTOR)?
        .ok_or_else(|| anyhow::anyhow!("Couldn't get sys actor state"))?;
    let sys_state: SystemStateNew = store
        .get_obj(&sys_actor.state)?
        .ok_or_else(|| anyhow::anyhow!("Couldn't get statev10"))?;

    let manifest = Manifest::load(&store, &sys_state.builtin_actors, 1)?;

    let eam_actor = ActorState::new_empty(*manifest.get_eam_code(), None);
    actors_out.set_actor(&Address::ETHEREUM_ACCOUNT_MANAGER_ACTOR, eam_actor)?;
    Ok(())
}
