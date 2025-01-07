// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

use crate::networks::{ChainConfig, Height};
use crate::shim::machine::BuiltinActorManifest;
use crate::shim::{
    address::Address,
    clock::ChainEpoch,
    state_tree::{StateTree, StateTreeVersion},
};
use anyhow::anyhow;
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::CborStore as _;

use super::{
    eam::EamPostMigrator, eth_account::EthAccountPostMigrator, init, system, verifier::Verifier,
    SystemStateOld,
};
use crate::state_migration::common::{migrators::nil_migrator, StateMigration};
impl<BS: Blockstore> StateMigration<BS> {
    pub fn add_nv18_migrations(
        &mut self,
        store: Arc<BS>,
        state: &Cid,
        new_manifest: &Cid,
    ) -> anyhow::Result<()> {
        let state_tree = StateTree::new_from_root(store.clone(), state)?;
        let system_actor = state_tree
            .get_actor(&Address::new_id(0))?
            .ok_or_else(|| anyhow!("system actor not found"))?;

        let system_actor_state = store
            .get_cbor::<SystemStateOld>(&system_actor.state)?
            .ok_or_else(|| anyhow!("system actor state not found"))?;
        let current_manifest =
            BuiltinActorManifest::load_v1_actor_list(&store, &system_actor_state.builtin_actors)?;

        let new_manifest = BuiltinActorManifest::load_manifest(&store, new_manifest)?;

        for (actor, cid) in current_manifest.builtin_actors() {
            let new_cid = new_manifest.get(actor)?;
            self.add_migrator(cid, nil_migrator(new_cid));
        }

        self.add_migrator(
            current_manifest.get_init(),
            init::init_migrator(new_manifest.get_init()),
        );

        self.add_migrator(
            current_manifest.get_system(),
            system::system_migrator(&new_manifest),
        );

        // Add post-migration steps
        self.add_post_migrator(Arc::new(EamPostMigrator));

        self.add_post_migrator(Arc::new(EthAccountPostMigrator));

        Ok(())
    }
}

/// Runs the migration for `NV18`. Returns the new state root.
pub fn run_migration<DB>(
    chain_config: &ChainConfig,
    blockstore: &Arc<DB>,
    state: &Cid,
    epoch: ChainEpoch,
) -> anyhow::Result<Cid>
where
    DB: Blockstore + Send + Sync,
{
    let new_manifest_cid = chain_config
        .height_infos
        .get(&Height::Hygge)
        .ok_or_else(|| anyhow!("no height info for network version NV18"))?
        .bundle
        .as_ref()
        .ok_or_else(|| anyhow!("no bundle info for network version NV18"))?;

    blockstore.get(new_manifest_cid)?.ok_or_else(|| {
        anyhow!(
            "manifest for network version NV18 not found in blockstore: {}",
            new_manifest_cid
        )
    })?;

    // Add migration specification verification
    let verifier = Arc::new(Verifier::default());

    let mut migration = StateMigration::<DB>::new(Some(verifier));
    migration.add_nv18_migrations(blockstore.clone(), state, new_manifest_cid)?;

    let actors_in = StateTree::new_from_root(blockstore.clone(), state)?;
    let actors_out = StateTree::new(blockstore.clone(), StateTreeVersion::V5)?;
    let new_state = migration.migrate_state_tree(blockstore, epoch, actors_in, actors_out)?;

    Ok(new_state)
}
