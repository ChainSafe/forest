// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

use anyhow::anyhow;
use cid::Cid;
use forest_networks::ChainConfig;
use forest_shim::{
    clock::ChainEpoch,
    state_tree::{StateTree, StateTreeVersion},
    version::NetworkVersion,
};
use fvm_ipld_blockstore::Blockstore;

use super::{eam::create_eam_actor, eth_account::create_eth_account_actor, verifier::Verifier};
use crate::{PostMigrationAction, StateMigration};

pub fn run_migration<DB>(
    chain_config: &ChainConfig,
    blockstore: &DB,
    state: &Cid,
    epoch: ChainEpoch,
) -> anyhow::Result<Cid>
where
    DB: 'static + Blockstore + Clone + Send + Sync,
{
    let new_manifest_cid = chain_config
        .manifests
        .get(&NetworkVersion::V18)
        .ok_or_else(|| anyhow!("no manifest for network version NV18"))?;

    blockstore.get(new_manifest_cid)?.ok_or_else(|| {
        anyhow!(
            "manifest for network version NV18 not found in blockstore: {}",
            new_manifest_cid
        )
    })?;

    // Add migration specification verification
    let verifier = Arc::new(Verifier::default());

    // Add post-migration steps
    let post_migration_actions = [create_eam_actor, create_eth_account_actor]
        .into_iter()
        .map(|action| Arc::new(action) as PostMigrationAction<DB>)
        .collect();

    let mut migration = StateMigration::<DB>::new(Some(verifier), post_migration_actions);
    migration.add_nv18_migrations(blockstore.clone(), state, new_manifest_cid)?;

    let actors_in = StateTree::new_from_root(blockstore.clone(), state)?;
    let actors_out = StateTree::new(blockstore.clone(), StateTreeVersion::V5)?;
    let new_state =
        migration.migrate_state_tree(blockstore.clone(), epoch, actors_in, actors_out)?;

    Ok(new_state)
}
