// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

use crate::networks::{ChainConfig, Height};
use crate::shim::{
    address::Address,
    clock::ChainEpoch,
    machine::{Manifest, MINER_ACTOR_NAME, POWER_ACTOR_NAME},
    state_tree::{StateTree, StateTreeVersion},
};
use anyhow::anyhow;
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::CborStore;

use super::{miner, power, system, verifier::Verifier, SystemStateOld};
use crate::state_migration::common::{migrators::nil_migrator, StateMigration};

impl<BS: Blockstore + Clone + Send + Sync> StateMigration<BS> {
    pub fn add_nv19_migrations(
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

        let current_manifest =
            Manifest::load_with_actors(&store, &system_actor_state.builtin_actors, 1)?;

        let new_manifest = Manifest::load(&store, new_manifest)?;

        for (name, code) in current_manifest.builtin_actors() {
            let new_code = new_manifest.code_by_name(name)?;
            self.add_migrator(*code, nil_migrator(*new_code));
        }

        self.add_migrator(
            *current_manifest.code_by_name(MINER_ACTOR_NAME)?,
            miner::miner_migrator(*new_manifest.code_by_name(MINER_ACTOR_NAME)?),
        );

        self.add_migrator(
            *current_manifest.code_by_name(POWER_ACTOR_NAME)?,
            power::power_migrator(*new_manifest.code_by_name(POWER_ACTOR_NAME)?),
        );

        self.add_migrator(
            *current_manifest.system_code(),
            system::system_migrator(&new_manifest),
        );

        Ok(())
    }
}

/// Runs the migration for `NV19`. Returns the new state root.
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
        .get(Height::Lightning as usize)
        .ok_or_else(|| anyhow!("no height info for network version NV19"))?
        .bundle
        .as_ref()
        .ok_or_else(|| anyhow!("no bundle info for network version NV19"))?;

    blockstore.get(new_manifest_cid)?.ok_or_else(|| {
        anyhow!(
            "manifest for network version NV19 not found in blockstore: {}",
            new_manifest_cid
        )
    })?;

    // Add migration specification verification
    let verifier = Arc::new(Verifier::default());

    let mut migration = StateMigration::<DB>::new(Some(verifier));
    migration.add_nv19_migrations(blockstore.clone(), state, new_manifest_cid)?;

    let actors_in = StateTree::new_from_root(blockstore.clone(), state)?;
    let actors_out = StateTree::new(blockstore.clone(), StateTreeVersion::V5)?;
    let new_state =
        migration.migrate_state_tree(blockstore.clone(), epoch, actors_in, actors_out)?;

    Ok(new_state)
}
