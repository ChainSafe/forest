// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
//
//! This module contains the migration logic for the `nv25fix` upgrade. See the parent module for
//! details on the fix.

use std::sync::Arc;

use crate::networks::{ChainConfig, Height};
use crate::shim::{
    address::Address,
    clock::ChainEpoch,
    machine::BuiltinActorManifest,
    state_tree::{StateTree, StateTreeVersion},
};
use crate::utils::db::CborStoreExt as _;
use anyhow::{ensure, Context as _};
use cid::Cid;

use fvm_ipld_blockstore::Blockstore;

use super::{system, verifier::Verifier, SystemStateOld};
use crate::state_migration::common::{migrators::nil_migrator, StateMigration};

impl<BS: Blockstore> StateMigration<BS> {
    pub fn add_nv25fix_migrations(
        &mut self,
        store: &Arc<BS>,
        state: &Cid,
        new_manifest: &BuiltinActorManifest,
    ) -> anyhow::Result<()> {
        let state_tree = StateTree::new_from_root(store.clone(), state)?;
        let system_actor = state_tree.get_required_actor(&Address::SYSTEM_ACTOR)?;
        let system_actor_state = store.get_cbor_required::<SystemStateOld>(&system_actor.state)?;

        let current_manifest_data = system_actor_state.builtin_actors;

        let current_manifest =
            BuiltinActorManifest::load_v1_actor_list(store, &current_manifest_data)?;

        for (name, code) in current_manifest.builtin_actors() {
            let new_code = new_manifest.get(name)?;
            self.add_migrator(code, nil_migrator(new_code))
        }

        self.add_migrator(
            current_manifest.get_system(),
            system::system_migrator(new_manifest),
        );

        Ok(())
    }
}

/// Runs the migration for `nv25fix`. Returns the new state root.
pub fn run_migration<DB>(
    chain_config: &ChainConfig,
    blockstore: &Arc<DB>,
    state: &Cid,
    epoch: ChainEpoch,
) -> anyhow::Result<Cid>
where
    DB: Blockstore + Send + Sync,
{
    // Technically the manifest for this just won't be there for mainnet, but better safe than
    // sorry.
    // check if testnet
    ensure!(
        chain_config.network.is_testnet(),
        "this fix migration is only for testnet"
    );
    let new_manifest_cid = chain_config
        .height_infos
        .get(&Height::TeepFix)
        .context("no height info for network version nv25fix")?
        .bundle
        .as_ref()
        .context("no bundle for network version nv25fix")?;

    blockstore.get(new_manifest_cid)?.context(format!(
        "manifest for network version nv25fix not found in blockstore: {new_manifest_cid}"
    ))?;

    // Add migration specification verification
    let verifier = Arc::new(Verifier::default());

    let new_manifest = BuiltinActorManifest::load_manifest(blockstore, new_manifest_cid)?;
    let mut migration = StateMigration::<DB>::new(Some(verifier));
    migration.add_nv25fix_migrations(blockstore, state, &new_manifest)?;

    let actors_in = StateTree::new_from_root(blockstore.clone(), state)?;
    let actors_out = StateTree::new(blockstore.clone(), StateTreeVersion::V5)?;
    let new_state = migration.migrate_state_tree(blockstore, epoch, actors_in, actors_out)?;

    Ok(new_state)
}
