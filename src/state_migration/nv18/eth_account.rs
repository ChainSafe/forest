// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::shim::{
    address::Address,
    machine::{Manifest, ETH_ACCOUNT_ACTOR_NAME},
    state_tree::{ActorState, StateTree},
};
use anyhow::anyhow;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::CborStore;

use crate::state_migration::common::PostMigrator;

use super::SystemStateNew;

pub struct EthAccountPostMigrator;

impl<BS: Blockstore> PostMigrator<BS> for EthAccountPostMigrator {
    /// Creates the Ethereum Account actor in the state tree.
    fn post_migrate_state(&self, store: &BS, actors_out: &mut StateTree<BS>) -> anyhow::Result<()> {
        let init_actor = actors_out
            .get_actor(&Address::INIT_ACTOR)?
            .ok_or_else(|| anyhow::anyhow!("Couldn't get init actor state"))?;
        let init_state: fil_actor_init_state::v10::State = store
            .get_cbor(&init_actor.state)?
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
            .get_cbor::<SystemStateNew>(&system_actor.state)?
            .ok_or_else(|| anyhow!("failed to get system actor state"))?;

        let new_manifest =
            Manifest::load_with_actors(&store, &system_actor_state.builtin_actors, 1)?;

        let eth_account_actor = ActorState::new(
            *new_manifest.code_by_name(ETH_ACCOUNT_ACTOR_NAME)?,
            fil_actors_shared::v10::runtime::EMPTY_ARR_CID,
            Default::default(),
            0,
            Some(eth_zero_addr),
        );

        actors_out.set_actor(&eth_zero_addr_id.into(), eth_account_actor)?;
        Ok(())
    }
}
