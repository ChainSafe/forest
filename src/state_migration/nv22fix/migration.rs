// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
//
//! This module contains the migration logic for the `NV22fix` upgrade.
//! Corresponding
use std::sync::Arc;

use super::{SystemStateOld, system, verifier::Verifier};
use crate::networks::{ChainConfig, Height, NetworkChain};
use crate::shim::{
    address::Address,
    clock::ChainEpoch,
    machine::{BuiltinActor, BuiltinActorManifest},
    state_tree::{StateTree, StateTreeVersion},
};
use crate::state_migration::common::PostMigrationCheck;
use crate::state_migration::common::{StateMigration, migrators::nil_migrator};
use crate::utils::db::CborStoreExt as _;
use anyhow::{Context, bail};
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;

impl<BS: Blockstore> StateMigration<BS> {
    pub fn add_nv22fix_migrations(
        &mut self,
        store: &Arc<BS>,
        state: &Cid,
        new_manifest: &BuiltinActorManifest,
    ) -> anyhow::Result<()> {
        let state_tree = StateTree::new_from_root(store.clone(), state)?;
        let system_actor = state_tree.get_required_actor(&Address::new_id(0))?;

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

struct PostMigrationVerifier {
    state_pre: Cid,
}

impl<BS: Blockstore> PostMigrationCheck<BS> for PostMigrationVerifier {
    fn post_migrate_check(&self, store: &BS, actors_out: &StateTree<BS>) -> anyhow::Result<()> {
        let actors_in = StateTree::new_from_root(Arc::new(store), &self.state_pre)?;
        let system_actor = actors_in.get_required_actor(&Address::new_id(0))?;

        let system_actor_state = store.get_cbor_required::<SystemStateOld>(&system_actor.state)?;

        let current_manifest_data = system_actor_state.builtin_actors;

        let current_manifest =
            BuiltinActorManifest::load_v1_actor_list(store, &current_manifest_data)?;

        actors_in.for_each(|address, actor_in| {
            let actor_out = actors_out.get_required_actor(&address)?;

            if actor_in.sequence != actor_out.sequence {
                bail!(
                    "actor {address} sequence mismatch: pre-migration: {}, post-migration: {}",
                    actor_in.sequence,
                    actor_out.sequence
                );
            }

            if actor_in.balance != actor_out.balance {
                bail!(
                    "actor {address} balance mismatch: pre-migration: {}, post-migration: {}",
                    actor_in.balance,
                    actor_out.balance
                );
            }

            if actor_in.state != actor_out.state && actor_in.code != current_manifest.get_system() {
                bail!(
                    "actor {address} state mismatch: pre-migration: {}, post-migration: {}",
                    actor_in.state,
                    actor_out.state
                );
            }

            if actor_in.code != current_manifest.get(BuiltinActor::VerifiedRegistry)?
                && actor_in.code != actor_out.code
            {
                bail!(
                    "actor {address} code mismatch: pre-migration: {}, post-migration: {}",
                    actor_in.code,
                    actor_out.code
                );
            }

            Ok(())
        })?;

        Ok(())
    }
}

/// Runs the migration for `NV22`. Returns the new state root.
pub fn run_migration<DB>(
    chain_config: &ChainConfig,
    blockstore: &Arc<DB>,
    state: &Cid,
    epoch: ChainEpoch,
) -> anyhow::Result<Cid>
where
    DB: Blockstore + Send + Sync,
{
    assert_eq!(
        chain_config.network,
        NetworkChain::Calibnet,
        "NV22fix only supported on Calibnet"
    );

    let new_manifest_cid = chain_config
        .height_infos
        .get(&Height::DragonFix)
        .context("no height info for network version NV22")?
        .bundle
        .as_ref()
        .context("no bundle for network version NV22")?;

    blockstore.get(new_manifest_cid)?.context(format!(
        "manifest for network version NV22fix not found in blockstore: {new_manifest_cid}"
    ))?;

    // Add migration specification verification
    let verifier = Arc::new(Verifier::default());

    let new_manifest = BuiltinActorManifest::load_manifest(blockstore, new_manifest_cid)?;
    let mut migration = StateMigration::<DB>::new(Some(verifier));
    migration.add_nv22fix_migrations(blockstore, state, &new_manifest)?;
    migration.add_post_migration_check(Arc::new(PostMigrationVerifier { state_pre: *state }));

    let actors_in = StateTree::new_from_root(blockstore.clone(), state)?;
    let actors_out = StateTree::new(blockstore.clone(), StateTreeVersion::V5)?;
    let new_state = migration.migrate_state_tree(blockstore, epoch, actors_in, actors_out)?;

    Ok(new_state)
}
