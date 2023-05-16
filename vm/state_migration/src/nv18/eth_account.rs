// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::anyhow;
use forest_shim::{
    address::Address,
    machine::ManifestV3,
    state_tree::{ActorState, StateTree},
};
use forest_utils::db::BlockstoreExt;
use fvm_ipld_blockstore::Blockstore;

use super::SystemStateNew;

/// Creates the Ethereum Account actor in the state tree.
pub fn create_eth_account_actor<BS: Blockstore + Clone + Send + Sync>(
    store: &BS,
    actors_out: &mut StateTree<BS>,
) -> anyhow::Result<()> {
    let init_actor = actors_out
        .get_actor(&Address::INIT_ACTOR)?
        .ok_or_else(|| anyhow::anyhow!("Couldn't get init actor state"))?;
    let init_state: fil_actor_init_v10::State = store
        .get_obj(&init_actor.state)?
        .ok_or_else(|| anyhow::anyhow!("Couldn't get statev10"))?;

    let eth_zero_addr =
        Address::new_delegated(Address::ETHEREUM_ACCOUNT_MANAGER_ACTOR.id()?, &[0; 20])?;
    let eth_zero_addr_id = init_state
        .resolve_address(&store, &eth_zero_addr.into())?
        .ok_or_else(|| anyhow!("failed to get eth zero actor"))?;

    let system_actor = actors_out
        .get_actor(&Address::new_id(0))?
        .ok_or_else(|| anyhow!("failed to get system actor"))?;

    let system_actor_state = store
        .get_obj::<SystemStateNew>(&system_actor.state)?
        .ok_or_else(|| anyhow!("failed to get system actor state"))?;

    let manifest_data = system_actor_state.builtin_actors;
    let new_manifest = ManifestV3::load(&store, &manifest_data, 1)?;

    let eth_account_actor = ActorState::new(
        *new_manifest.get_ethaccount_code(),
        fil_actors_runtime_v10::runtime::EMPTY_ARR_CID,
        Default::default(),
        0,
        Some(eth_zero_addr),
    );

    actors_out.set_actor(&eth_zero_addr_id.into(), eth_account_actor)?;
    Ok(())
}
