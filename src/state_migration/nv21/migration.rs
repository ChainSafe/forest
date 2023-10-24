// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

use crate::networks::{ChainConfig, Height, NetworkChain};
use crate::shim::{
    address::Address,
    clock::ChainEpoch,
    machine::{BuiltinActor, BuiltinActorManifest},
    sector::{RegisteredPoStProofV3, RegisteredSealProofV3},
    state_tree::{StateTree, StateTreeVersion},
};
use anyhow::Context;
use cid::Cid;
use fil_actors_shared::v11::runtime::ProofSet;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::CborStore;

use super::{miner, system, verifier::Verifier, SystemStateOld};
use crate::state_migration::common::{migrators::nil_migrator, StateMigration};

impl<BS: Blockstore> StateMigration<BS> {
    pub fn add_nv21_migrations(
        &mut self,
        store: &Arc<BS>,
        state: &Cid,
        new_manifest: &BuiltinActorManifest,
        chain_config: &ChainConfig,
    ) -> anyhow::Result<()> {
        let state_tree = StateTree::new_from_root(store.clone(), state)?;
        let system_actor = state_tree
            .get_actor(&Address::new_id(0))?
            .context("failed to get system actor")?;

        let system_actor_state = store
            .get_cbor::<SystemStateOld>(&system_actor.state)?
            .context("system actor state not found")?;

        let current_manifest_data = system_actor_state.builtin_actors;

        let current_manifest =
            BuiltinActorManifest::load_v1_actor_list(store, &current_manifest_data)?;

        for (name, code) in current_manifest.builtin_actors() {
            let new_code = new_manifest.get(name)?;
            self.add_migrator(code, nil_migrator(new_code))
        }

        let (policy_old, policy_new) = match &chain_config.network {
            NetworkChain::Mainnet => (
                fil_actors_shared::v11::runtime::Policy::mainnet(),
                fil_actors_shared::v12::runtime::Policy::mainnet(),
            ),
            NetworkChain::Calibnet => (
                fil_actors_shared::v11::runtime::Policy::calibnet(),
                fil_actors_shared::v12::runtime::Policy::calibnet(),
            ),
            NetworkChain::Devnet(_) => {
                let mut policy_old = fil_actors_shared::v11::runtime::Policy::mainnet();
                policy_old.minimum_consensus_power = 2048.into();
                policy_old.minimum_verified_allocation_size = 256.into();
                policy_old.pre_commit_challenge_delay = 10;

                let mut proofs = ProofSet::default_seal_proofs();
                proofs.insert(RegisteredSealProofV3::StackedDRG2KiBV1);
                proofs.insert(RegisteredSealProofV3::StackedDRG8MiBV1);
                policy_old.valid_pre_commit_proof_type = proofs;

                let mut proofs = ProofSet::default_post_proofs();
                proofs.insert(RegisteredPoStProofV3::StackedDRGWindow2KiBV1);
                proofs.insert(RegisteredPoStProofV3::StackedDRGWindow8MiBV1);
                policy_old.valid_post_proof_type = proofs;

                (
                    policy_old,
                    fil_actors_shared::v12::runtime::Policy::devnet(),
                )
            }
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
        .get(Height::Watermelon as usize)
        .context("no height info for network version NV21")?
        .bundle
        .as_ref()
        .context("no bundle for network version NV21")?;

    blockstore.get(new_manifest_cid)?.context(format!(
        "manifest for network version NV21 not found in blockstore: {new_manifest_cid}"
    ))?;

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
