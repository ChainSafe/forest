// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

use anyhow::anyhow;
use cid::Cid;
use forest_networks::{ChainConfig, Height};
use forest_shim::{
    address::Address,
    clock::ChainEpoch,
    state_tree::{StateTree, StateTreeVersion},
};
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::CborStore;

use super::{
    eam::EamPostMigrator, eth_account::EthAccountPostMigrator, init, system, verifier::Verifier,
    ManifestNew, ManifestOld, SystemStateOld,
};
use crate::common::{migrators::nil_migrator, StateMigration};
impl<BS: Blockstore + Clone + Send + Sync> StateMigration<BS> {
    pub fn add_nv18_migrations(
        &mut self,
        store: BS,
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
        let current_manifest_data = system_actor_state.builtin_actors;
        let current_manifest = ManifestOld::load(&store, &current_manifest_data, 1)?;

        let (version, new_manifest_data): (u32, Cid) = store
            .get_cbor(new_manifest)?
            .ok_or_else(|| anyhow!("new manifest not found"))?;
        let new_manifest = ManifestNew::load(&store, &new_manifest_data, version)?;

        current_manifest.builtin_actor_codes().for_each(|code| {
            let id = current_manifest.id_by_code(code);
            let new_code = new_manifest.code_by_id(id).unwrap();
            self.add_migrator(*code, nil_migrator(*new_code));
        });

        self.add_migrator(
            *current_manifest.get_init_code(),
            init::init_migrator(*new_manifest.get_init_code()),
        );

        self.add_migrator(
            *current_manifest.get_system_code(),
            system::system_migrator(new_manifest_data, *new_manifest.get_system_code()),
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
    blockstore: &DB,
    state: &Cid,
    epoch: ChainEpoch,
) -> anyhow::Result<Cid>
where
    DB: 'static + Blockstore + Clone + Send + Sync,
{
    let new_manifest_cid = chain_config
        .height_infos
        .get(Height::Hygge as usize)
        .ok_or_else(|| anyhow!("no height info for network version NV18"))?
        .bundle
        .as_ref()
        .ok_or_else(|| anyhow!("no bundle info for network version NV18"))?
        .manifest;

    blockstore.get(&new_manifest_cid)?.ok_or_else(|| {
        anyhow!(
            "manifest for network version NV18 not found in blockstore: {}",
            new_manifest_cid
        )
    })?;

    // Add migration specification verification
    let verifier = Arc::new(Verifier::default());

    let mut migration = StateMigration::<DB>::new(Some(verifier));
    migration.add_nv18_migrations(blockstore.clone(), state, &new_manifest_cid)?;

    let actors_in = StateTree::new_from_root(blockstore.clone(), state)?;
    let actors_out = StateTree::new(blockstore.clone(), StateTreeVersion::V5)?;
    let new_state =
        migration.migrate_state_tree(blockstore.clone(), epoch, actors_in, actors_out)?;

    Ok(new_state)
}
