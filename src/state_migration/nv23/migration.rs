// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
//
//! This module contains the migration logic for the `NV23` upgrade.

use std::sync::Arc;

use crate::networks::{ChainConfig, Height};
use crate::shim::{
    address::Address,
    clock::ChainEpoch,
    machine::{BuiltinActor, BuiltinActorManifest},
    state_tree::{StateTree, StateTreeVersion},
};
use crate::utils::db::CborStoreExt as _;
use anyhow::Context as _;
use cid::Cid;

use fvm_ipld_blockstore::Blockstore;

use super::mining_reserve::MiningReservePostMigrator;
use super::{SystemStateOld, system, verifier::Verifier};
use crate::state_migration::common::{StateMigration, migrators::nil_migrator};

impl<BS: Blockstore> StateMigration<BS> {
    pub fn add_nv23_migrations(
        &mut self,
        store: &Arc<BS>,
        state: &Cid,
        new_manifest: &BuiltinActorManifest,
        _chain_config: &ChainConfig,
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

        self.add_post_migrator(Arc::new(MiningReservePostMigrator {
            new_account_code_cid: new_manifest.get(BuiltinActor::Account)?,
            new_multisig_code_cid: new_manifest.get(BuiltinActor::Multisig)?,
        }));

        Ok(())
    }
}

/// Runs the migration for `NV23`. Returns the new state root.
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
        .get(&Height::Waffle)
        .context("no height info for network version NV23")?
        .bundle
        .as_ref()
        .context("no bundle for network version NV23")?;

    blockstore.get(new_manifest_cid)?.context(format!(
        "manifest for network version NV23 not found in blockstore: {new_manifest_cid}"
    ))?;

    // Add migration specification verification
    let verifier = Arc::new(Verifier::default());

    let new_manifest = BuiltinActorManifest::load_manifest(blockstore, new_manifest_cid)?;
    let mut migration = StateMigration::<DB>::new(Some(verifier));
    migration.add_nv23_migrations(blockstore, state, &new_manifest, chain_config)?;

    let actors_in = StateTree::new_from_root(blockstore.clone(), state)?;
    let actors_out = StateTree::new(blockstore.clone(), StateTreeVersion::V5)?;
    let new_state = migration.migrate_state_tree(blockstore, epoch, actors_in, actors_out)?;

    Ok(new_state)
}
