// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
//
//! This module contains the migration logic for the `NV22` upgrade. State migration logic
//! comes from the
//! [FIP-0076](https://github.com/filecoin-project/FIPs/blob/master/FIPS/fip-0076.md#migration).

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

use super::{market, miner, system, verifier::Verifier, SystemStateOld};
use crate::state_migration::common::{migrators::nil_migrator, StateMigration};

impl<BS: Blockstore> StateMigration<BS> {
    pub fn add_nv22_migrations(
        &mut self,
        store: &Arc<BS>,
        state: &Cid,
        new_manifest: &BuiltinActorManifest,
        chain_config: &ChainConfig,
    ) -> anyhow::Result<()> {
        let upgrade_epoch = chain_config
            .height_infos
            .get(&Height::Dragon)
            .context("no height info for network version NV22")?
            .epoch;

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

        let miner_old_code = current_manifest.get(BuiltinActor::Miner)?;
        let miner_new_code = new_manifest.get(BuiltinActor::Miner)?;

        let market_old_code = current_manifest.get(BuiltinActor::Market)?;
        let market_new_code = new_manifest.get(BuiltinActor::Market)?;

        let provider_sectors = Arc::new(miner::ProviderSectors::default());

        self.add_migrator(
            miner_old_code,
            miner::miner_migrator(upgrade_epoch, provider_sectors.clone(), miner_new_code)?,
        );

        self.add_migrator(
            market_old_code,
            market::market_migrator(upgrade_epoch, provider_sectors.clone(), market_new_code)?,
        );

        self.add_migrator(
            current_manifest.get_system(),
            system::system_migrator(new_manifest),
        );

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
    let new_manifest_cid = chain_config
        .height_infos
        .get(&Height::Dragon)
        .context("no height info for network version NV22")?
        .bundle
        .as_ref()
        .context("no bundle for network version NV22")?;

    blockstore.get(new_manifest_cid)?.context(format!(
        "manifest for network version NV22 not found in blockstore: {new_manifest_cid}"
    ))?;

    // Add migration specification verification
    let verifier = Arc::new(Verifier::default());

    let new_manifest = BuiltinActorManifest::load_manifest(blockstore, new_manifest_cid)?;
    let mut migration = StateMigration::<DB>::new(Some(verifier));
    migration.add_nv22_migrations(blockstore, state, &new_manifest, chain_config)?;

    let actors_in = StateTree::new_from_root(blockstore.clone(), state)?;
    let actors_out = StateTree::new(blockstore.clone(), StateTreeVersion::V5)?;
    let new_state = migration.migrate_state_tree(blockstore, epoch, actors_in, actors_out)?;

    Ok(new_state)
}
