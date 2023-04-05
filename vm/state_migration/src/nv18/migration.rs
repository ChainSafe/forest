// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

use anyhow::anyhow;
use cid::Cid;
use forest_shim::{
    address::Address,
    clock::ChainEpoch,
    state_tree::{ActorState, StateTree, StateTreeVersion},
};
use forest_utils::db::BlockstoreExt;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::CborStore;

use super::{calibnet, eam::create_eam_actor, verifier::Verifier};
use crate::{PostMigrationAction, StateMigration};

pub fn run_migration<DB>(blockstore: &DB, state: &Cid, epoch: ChainEpoch) -> anyhow::Result<Cid>
where
    DB: 'static + Blockstore + Clone + Send + Sync,
{
    let state_tree = StateTree::new_from_root(blockstore, state)?;

    let new_manifest_cid =
        Cid::try_from("bafy2bzaced25ta3j6ygs34roprilbtb3f6mxifyfnm7z7ndquaruxzdq3y7lo")?;
    let (_, new_manifest_data): (u32, Cid) = state_tree
        .store()
        .get_cbor(&new_manifest_cid)?
        .ok_or_else(|| anyhow!("could not find old state migration manifest"))?;

    let verifier = Arc::new(Verifier::default());
    let create_eam_actor = |_store: &DB, actors_out: &mut StateTree<DB>| {
        let eam_actor = create_eam_actor();
        actors_out.set_actor(&Address::ETHEREUM_ACCOUNT_MANAGER_ACTOR, eam_actor)?;
        Ok(())
    };
    let create_eth_account_actor = |store: &DB, actors_out: &mut StateTree<DB>| {
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

        let eth_account_actor = ActorState::new(
            *calibnet::v10::ETH_ACCOUNT,
            fil_actors_runtime_v10::runtime::EMPTY_ARR_CID,
            Default::default(),
            0,
            Some(eth_zero_addr),
        );

        actors_out.set_actor(&eth_zero_addr_id.into(), eth_account_actor)?;
        Ok(())
    };
    let post_migration_actions = [create_eam_actor, create_eth_account_actor]
        .into_iter()
        .map(|action| Arc::new(action) as PostMigrationAction<DB>)
        .collect();

    let mut migration =
        StateMigration::<DB>::new(new_manifest_data, Some(verifier), post_migration_actions);
    migration.add_nil_migrations();
    migration.add_nv_18_migrations();

    let actors_in = StateTree::new_from_root(blockstore.clone(), state)?;
    let actors_out = StateTree::new(blockstore.clone(), StateTreeVersion::V5)?;
    let new_state =
        migration.migrate_state_tree(blockstore.clone(), epoch, actors_in, actors_out)?;

    Ok(new_state)
}
