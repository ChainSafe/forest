// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

use crate::networks::{ChainConfig, Height};
use crate::shim::machine::*;
use crate::shim::{
    address::Address,
    clock::ChainEpoch,
    machine::Manifest,
    state_tree::{StateTree, StateTreeVersion},
};
use anyhow::{anyhow, Context};
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::CborStore;

use super::super::common::{
    migrators::{nil_migrator, DeferredMigrator},
    StateMigration,
};
use super::{
    datacap, miner, system, util::get_pending_verified_deals_and_total_size, verifier::Verifier,
    verifreg_market::VerifregMarketPostMigrator, SystemStateOld,
};

impl<BS: Blockstore + Clone + Send + Sync> StateMigration<BS> {
    pub fn add_nv17_migrations(
        &mut self,
        store: BS,
        actors_in: &mut StateTree<BS>,
        new_manifest: &Manifest,
        prior_epoch: ChainEpoch,
        chain_config: &ChainConfig,
    ) -> anyhow::Result<()> {
        let system_actor = actors_in
            .get_actor(&Address::new_id(0))?
            .ok_or_else(|| anyhow!("system actor not found"))?;

        let system_actor_state: SystemStateOld = store
            .get_cbor(&system_actor.state)?
            .ok_or_else(|| anyhow!("system actor state not found"))?;
        let current_manifest_data = system_actor_state.builtin_actors;

        let current_manifest = Manifest::load_with_actors(&store, &current_manifest_data, 1)?;

        let verifreg_actor_v8 = actors_in
            .get_actor(&fil_actors_shared::v8::VERIFIED_REGISTRY_ACTOR_ADDR.into())?
            .context("Failed to load verifreg actor v8")?;

        let market_actor_v8 = actors_in
            .get_actor(&fil_actors_shared::v8::STORAGE_MARKET_ACTOR_ADDR.into())?
            .context("Failed to load market actor v8")?;

        let market_state_v8: fil_actor_market_state::v8::State = store
            .get_cbor(&market_actor_v8.state)?
            .context("Failed to load market state v8")?;

        let init_actor_v8 = actors_in
            .get_actor(&fil_actors_shared::v8::INIT_ACTOR_ADDR.into())?
            .context("Failed to load init actor v8")?;

        let init_state_v8: fil_actor_init_state::v8::State = store
            .get_cbor(&init_actor_v8.state)?
            .context("Failed to load init state v8")?;

        let (pending_verified_deals, pending_verified_deal_size) =
            get_pending_verified_deals_and_total_size(&store, &market_state_v8)?;

        for (name, code) in current_manifest.builtin_actors() {
            if name == MARKET_ACTOR_NAME || name == VERIFREG_ACTOR_NAME {
                self.add_migrator(*code, Arc::new(DeferredMigrator))
            } else {
                let new_code = new_manifest.code_by_name(name)?;
                self.add_migrator(*code, nil_migrator(*new_code));
            }
        }

        // https://github.com/filecoin-project/go-state-types/blob/1e6cf0d47cdda75383ef036fc2725d1cf51dbde8/builtin/v9/migration/top.go#L178
        self.add_migrator(
            *current_manifest.system_code(),
            system::system_migrator(new_manifest),
        );

        let miner_v8_actor_code = current_manifest.code_by_name(MINER_ACTOR_NAME)?;
        let miner_v9_actor_code = new_manifest.code_by_name(MINER_ACTOR_NAME)?;

        self.add_migrator(
            *miner_v8_actor_code,
            miner::miner_migrator(
                *miner_v9_actor_code,
                &store,
                market_state_v8.proposals,
                chain_config,
            )?,
        );

        let verifreg_state_v8_cid = verifreg_actor_v8.state;
        let verifreg_state_v8: fil_actor_verifreg_state::v8::State = store
            .get_cbor(&verifreg_state_v8_cid)?
            .context("Failed to load verifreg state v8")?;
        let verifreg_code = *new_manifest.code_by_name(VERIFREG_ACTOR_NAME)?;
        let market_code = *new_manifest.code_by_name(MARKET_ACTOR_NAME)?;

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
            new_code_cid: *new_manifest.code_by_name(DATACAP_ACTOR_NAME)?,
            verifreg_state: store
                .get_cbor(&verifreg_state_v8_cid)?
                .context("Failed to load verifreg state v8")?,
            pending_verified_deal_size,
        }));

        Ok(())
    }
}

/// Runs the migration for `NV17`. Returns the new state root.
pub fn run_migration<DB>(
    chain_config: &ChainConfig,
    blockstore: &DB,
    state: &Cid,
    epoch: ChainEpoch,
) -> anyhow::Result<Cid>
where
    DB: Blockstore + Clone + Send + Sync,
{
    let new_manifest_cid = chain_config
        .height_infos
        .get(Height::Shark as usize)
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

    let new_manifest = Manifest::load(&blockstore, new_manifest_cid)?;

    let mut actors_in = StateTree::new_from_root(blockstore.clone(), state)?;

    // Add migration specification verification
    let verifier = Arc::new(Verifier::default());

    let mut migration = StateMigration::<DB>::new(Some(verifier));
    migration.add_nv17_migrations(
        blockstore.clone(),
        &mut actors_in,
        &new_manifest,
        epoch,
        chain_config,
    )?;

    let actors_out = StateTree::new(blockstore.clone(), StateTreeVersion::V4)?;

    let new_state =
        migration.migrate_state_tree(blockstore.clone(), epoch, actors_in, actors_out)?;

    Ok(new_state)
}
