// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
//
//! This module contains the migration logic for the `NV29` upgrade.

use super::market::MarketMigrator;
use super::reward::RewardMigrator;
use super::{SystemStateOld, system, verifier::Verifier};
use crate::networks::{ChainConfig, Height, SolsticeRewardBootstrapParams};
use crate::prelude::*;
use crate::shim::{
    address::Address,
    clock::ChainEpoch,
    machine::{BuiltinActor, BuiltinActorManifest},
    state_tree::{StateTree, StateTreeVersion},
};
use crate::state_migration::common::{StateMigration, migrators::nil_migrator};
use crate::utils::db::CborStoreExt as _;

impl<BS: Blockstore + ShallowClone> StateMigration<BS> {
    pub fn add_nv29_migrations(
        &mut self,
        store: &BS,
        state: &Cid,
        new_manifest: &BuiltinActorManifest,
        reward_bootstrap: &SolsticeRewardBootstrapParams,
        activation_epoch: ChainEpoch,
    ) -> anyhow::Result<()> {
        let state_tree = StateTree::new_from_root(store, state)?;
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
        self.add_migrator(
            current_manifest.get(BuiltinActor::Reward)?,
            Arc::new(RewardMigrator::new(
                reward_bootstrap,
                activation_epoch,
                new_manifest.get(BuiltinActor::Reward)?,
            )?),
        );
        self.add_migrator(
            current_manifest.get(BuiltinActor::Market)?,
            Arc::new(MarketMigrator {
                new_code_cid: new_manifest.get(BuiltinActor::Market)?,
            }),
        );

        Ok(())
    }
}

/// Runs the migration for `NV29`. Returns the new state root.
pub fn run_migration<DB>(
    chain_config: &ChainConfig,
    blockstore: &DB,
    state: &Cid,
    epoch: ChainEpoch,
) -> anyhow::Result<Cid>
where
    DB: Blockstore + ShallowClone + Send + Sync,
{
    let new_manifest_cid = chain_config
        .height_infos
        .get(&Height::Solstice)
        .context("no height info for network version NV29")?
        .bundle
        .as_ref()
        .context("no bundle for network version NV29")?;

    blockstore.get(new_manifest_cid)?.context(format!(
        "manifest for network version NV29 not found in blockstore: {new_manifest_cid}"
    ))?;

    // Add migration specification verification
    let verifier = Arc::new(Verifier::default());

    let new_manifest = BuiltinActorManifest::load_manifest(blockstore, new_manifest_cid)?;
    let mut migration = StateMigration::<DB>::new(Some(verifier));
    // The bootstrap streams start at the first epoch executed on the migrated state, like
    // go-state-types' `activationEpoch := priorEpoch + 1`.
    migration.add_nv29_migrations(
        blockstore,
        state,
        &new_manifest,
        &chain_config.solstice_reward_bootstrap,
        epoch + 1,
    )?;

    let actors_in = StateTree::new_from_root(blockstore, state)?;
    let actors_out = StateTree::new(blockstore, StateTreeVersion::V5)?;
    let new_state = migration.migrate_state_tree(blockstore, epoch, actors_in, actors_out)?;

    Ok(new_state)
}
