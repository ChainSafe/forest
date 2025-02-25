// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

use super::{SystemStateOld, miner, system, verifier::Verifier};
use crate::networks::{ChainConfig, Height, NetworkChain};
use crate::shim::{
    address::Address,
    clock::ChainEpoch,
    machine::{BuiltinActor, BuiltinActorManifest},
    sector::{RegisteredPoStProofV3, RegisteredSealProofV3},
    state_tree::{StateTree, StateTreeVersion},
};
use crate::state_migration::common::{StateMigration, migrators::nil_migrator};
use crate::utils::db::CborStoreExt as _;
use crate::{make_butterfly_policy, make_calibnet_policy, make_devnet_policy, make_mainnet_policy};
use anyhow::Context as _;
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;

impl<BS: Blockstore> StateMigration<BS> {
    pub fn add_nv21_migrations(
        &mut self,
        store: &Arc<BS>,
        state: &Cid,
        new_manifest: &BuiltinActorManifest,
        chain_config: &ChainConfig,
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

        let (policy_old, policy_new) = match &chain_config.network {
            NetworkChain::Mainnet => (make_mainnet_policy!(v11), make_mainnet_policy!(v12)),
            NetworkChain::Calibnet => (make_calibnet_policy!(v11), make_calibnet_policy!(v12)),
            NetworkChain::Butterflynet => {
                (make_butterfly_policy!(v11), make_butterfly_policy!(v12))
            }
            NetworkChain::Devnet(_) => (make_devnet_policy!(v11), make_devnet_policy!(v12)),
        };
        let miner_old_code = current_manifest.get(BuiltinActor::Miner)?;
        let miner_new_code = new_manifest.get(BuiltinActor::Miner)?;

        self.add_migrator(
            miner_old_code,
            miner::miner_migrator(&policy_old, &policy_new, store, miner_new_code)?,
        );

        self.add_migrator(
            current_manifest.get_system(),
            system::system_migrator(new_manifest),
        );

        Ok(())
    }
}

/// Runs the migration for `NV21`. Returns the new state root.
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
        .get(&Height::Watermelon)
        .context("no height info for network version NV21")?
        .bundle
        .as_ref()
        .context("no bundle for network version NV21")?;

    blockstore.get(new_manifest_cid)?.with_context(|| {
        format!("manifest for network version NV21 not found in blockstore: {new_manifest_cid}")
    })?;

    // Add migration specification verification
    let verifier = Arc::new(Verifier::default());

    let new_manifest = BuiltinActorManifest::load_manifest(blockstore, new_manifest_cid)?;
    let mut migration = StateMigration::<DB>::new(Some(verifier));
    migration.add_nv21_migrations(blockstore, state, &new_manifest, chain_config)?;

    let actors_in = StateTree::new_from_root(blockstore.clone(), state)?;
    let actors_out = StateTree::new(blockstore.clone(), StateTreeVersion::V5)?;
    let new_state = migration.migrate_state_tree(blockstore, epoch, actors_in, actors_out)?;

    Ok(new_state)
}
