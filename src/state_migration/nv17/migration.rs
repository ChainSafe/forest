// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

use crate::networks::{ChainConfig, Height, NetworkChain};
use crate::shim::{
    address::Address,
    clock::ChainEpoch,
    machine::{BuiltinActor, BuiltinActorManifest},
    state_tree::{StateTree, StateTreeVersion},
};
use crate::utils::db::CborStoreExt as _;
use anyhow::anyhow;
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::CborStore as _;

use super::super::common::{
    migrators::{nil_migrator, DeferredMigrator},
    StateMigration,
};
use super::{
    datacap, miner, system, util::get_pending_verified_deals_and_total_size, verifier::Verifier,
    verifreg_market::VerifregMarketPostMigrator, SystemStateOld,
};

impl<BS: Blockstore + Send + Sync> StateMigration<BS> {
    pub fn add_nv17_migrations(
        &mut self,
        store: &Arc<BS>,
        actors_in: &mut StateTree<BS>,
        new_manifest: &BuiltinActorManifest,
        prior_epoch: ChainEpoch,
        chain: NetworkChain,
    ) -> anyhow::Result<()> {
        let system_actor = actors_in
            .get_actor(&Address::new_id(0))?
            .ok_or_else(|| anyhow!("system actor not found"))?;

        let system_actor_state: SystemStateOld = store
            .get_cbor(&system_actor.state)?
            .ok_or_else(|| anyhow!("system actor state not found"))?;
        let current_manifest_data = system_actor_state.builtin_actors;

        let current_manifest =
            BuiltinActorManifest::load_v1_actor_list(store, &current_manifest_data)?;

        let verifreg_actor_v8 = actors_in
            .get_required_actor(&fil_actors_shared::v8::VERIFIED_REGISTRY_ACTOR_ADDR.into())?;

        let market_actor_v8 = actors_in
            .get_required_actor(&fil_actors_shared::v8::STORAGE_MARKET_ACTOR_ADDR.into())?;

        let market_state_v8: fil_actor_market_state::v8::State =
            store.get_cbor_required(&market_actor_v8.state)?;

        let init_actor_v8 =
            actors_in.get_required_actor(&fil_actors_shared::v8::INIT_ACTOR_ADDR.into())?;

        let init_state_v8: fil_actor_init_state::v8::State =
            store.get_cbor_required(&init_actor_v8.state)?;

        let (pending_verified_deals, pending_verified_deal_size) =
            get_pending_verified_deals_and_total_size(&store, &market_state_v8)?;

        for (actor, cid) in current_manifest.builtin_actors() {
            match actor {
                BuiltinActor::Market | BuiltinActor::VerifiedRegistry => {
                    self.add_migrator(cid, Arc::new(DeferredMigrator))
                }
                _ => {
                    let new_code = new_manifest.get(actor)?;
                    self.add_migrator(cid, nil_migrator(new_code))
                }
            }
        }

        // https://github.com/filecoin-project/go-state-types/blob/1e6cf0d47cdda75383ef036fc2725d1cf51dbde8/builtin/v9/migration/top.go#L178
        self.add_migrator(
            current_manifest.get_system(),
            system::system_migrator(new_manifest),
        );

        let miner_v8_actor_code = current_manifest.get(BuiltinActor::Miner)?;
        let miner_v9_actor_code = new_manifest.get(BuiltinActor::Miner)?;

        self.add_migrator(
            miner_v8_actor_code,
            miner::miner_migrator(miner_v9_actor_code, store, market_state_v8.proposals, chain)?,
        );

        let verifreg_state_v8_cid = verifreg_actor_v8.state;
        let verifreg_state_v8: fil_actor_verifreg_state::v8::State =
            store.get_cbor_required(&verifreg_state_v8_cid)?;
        let verifreg_code = new_manifest.get(BuiltinActor::VerifiedRegistry)?;
        let market_code = new_manifest.get(BuiltinActor::Market)?;

        self.add_post_migrator(Arc::new(VerifregMarketPostMigrator {
            prior_epoch,
            init_state_v8,
            market_state_v8,
            verifreg_state_v8,
            pending_verified_deals,
            verifreg_actor_v8,
            market_actor_v8,
            verifreg_code,
            market_code,
        }));

        // Note: The `datacap` actor is handled specially in Go code,
        // by setting up an empty actor to migrate from with a migrator,
        // while forest uses a post migrator to simplify the logic.
        self.add_post_migrator(Arc::new(datacap::DataCapPostMigrator {
            new_code_cid: new_manifest.get(BuiltinActor::DataCap)?,
            verifreg_state: store.get_cbor_required(&verifreg_state_v8_cid)?,
            pending_verified_deal_size,
        }));

        Ok(())
    }
}

/// Runs the migration for `NV17`. Returns the new state root.
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
        .get(&Height::Shark)
        .ok_or_else(|| anyhow!("no height info for network version NV17"))?
        .bundle
        .as_ref()
        .ok_or_else(|| anyhow!("no bundle info for network version NV17"))?;

    blockstore.get(new_manifest_cid)?.ok_or_else(|| {
        anyhow!(
            "manifest for network version NV17 not found in blockstore: {}",
            new_manifest_cid
        )
    })?;

    let new_manifest = BuiltinActorManifest::load_manifest(blockstore, new_manifest_cid)?;

    let mut actors_in = StateTree::new_from_root(blockstore.clone(), state)?;

    // Add migration specification verification
    let verifier = Arc::new(Verifier::default());

    let mut migration = StateMigration::<DB>::new(Some(verifier));
    migration.add_nv17_migrations(
        blockstore,
        &mut actors_in,
        &new_manifest,
        epoch,
        chain_config.network.clone(),
    )?;

    let actors_out = StateTree::new(blockstore.clone(), StateTreeVersion::V4)?;

    let new_state = migration.migrate_state_tree(blockstore, epoch, actors_in, actors_out)?;

    Ok(new_state)
}
