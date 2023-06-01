// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

use anyhow::{anyhow, Context};
use cid::Cid;
use fil_actor_interface::miner::{is_v8_miner_cid, is_v9_miner_cid};
use forest_networks::{ChainConfig, Height};
use forest_shim::{
    address::Address,
    clock::ChainEpoch,
    deal::DealID,
    state_tree::{ActorState, StateTree, StateTreeVersion},
};
use forest_utils::db::BlockstoreExt;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::CborStore;

use super::{
    datacap, miner, system, util::get_pending_verified_deals_and_total_size, verifier::Verifier,
    verifreg_market, ManifestNew, ManifestOld, SystemStateOld,
};
use crate::common::{migrators::nil_migrator, PostMigrationAction, StateMigration};

impl<BS: Blockstore + Clone + Send + Sync> StateMigration<BS> {
    pub fn add_nv17_migrations(
        &mut self,
        store: BS,
        state: &Cid,
        new_manifest: &Cid,
    ) -> anyhow::Result<()> {
        let state_tree = StateTree::new_from_root(store.clone(), state)?;
        let system_actor = state_tree
            .get_actor(&Address::new_id(0))?
            .ok_or_else(|| anyhow!("system actor not found"))?;

        let system_actor_state: SystemStateOld = store
            .get_cbor(&system_actor.state)?
            .ok_or_else(|| anyhow!("system actor state not found"))?;
        let current_manifest_data = system_actor_state.builtin_actors;
        let current_manifest = ManifestOld::load(&store, &current_manifest_data, 1)?;

        let (version, new_manifest_data): (u32, Cid) = store
            .get_cbor(new_manifest)?
            .ok_or_else(|| anyhow!("new manifest not found"))?;
        let new_manifest = ManifestNew::load(&store, &new_manifest_data, version)?;

        let verifreg_actor_v8 = state_tree
            .get_actor(&Address::new_id(
                fil_actors_shared::v8::VERIFIED_REGISTRY_ACTOR_ADDR.id()?,
            ))?
            .context("Failed to load verifreg actor v8")?;

        let verifreg_v8_state: fil_actor_verifreg_state::v8::State = store
            .get_cbor(&verifreg_actor_v8.state)?
            .context("Failed to load verifreg state v8")?;

        let market_actor_v8 = state_tree
            .get_actor(&Address::new_id(
                fil_actors_shared::v8::STORAGE_MARKET_ACTOR_ADDR.id()?,
            ))?
            .context("Failed to load market actor v8")?;

        let market_v8_state: fil_actor_market_state::v8::State = store
            .get_cbor(&market_actor_v8.state)?
            .context("Failed to load market state v8")?;

        let (pending_verified_deals, pending_verified_deal_size) =
            get_pending_verified_deals_and_total_size(&store, &market_v8_state)?;

        current_manifest.builtin_actor_codes().for_each(|code| {
            let id = current_manifest.id_by_code(code);
            let new_code = new_manifest.code_by_id(id).unwrap();
            self.add_migrator(*code, nil_migrator(*new_code));
        });

        //https://github.com/filecoin-project/go-state-types/blob/master/builtin/v9/migration/top.go#LL176C2-L176C38
        self.add_migrator(
            *current_manifest.get_system_code(),
            system::system_migrator(new_manifest_data, *new_manifest.get_system_code()),
        );

        let datacap_code = new_manifest
            .code_by_id(fil_actors_shared::v9::builtin::DATACAP_TOKEN_ACTOR_ID as _)
            .context("datacap code not found in new manifest")?;
        self.add_migrator(
            // Use the new code as prior code here, have set an empty actor in `run_migrations` to
            // migrate from
            *datacap_code,
            datacap::datacap_migrator(verifreg_v8_state, pending_verified_deal_size)?,
        );

        // On go side, cid is found by name `storageminer`, however, no equivilent API is available on rust side.
        let miner_v8_cid = current_manifest
            .builtin_actor_codes()
            .find(|cid| is_v8_miner_cid(cid))
            .context("Failed to retrieve miner v8 cid")?;
        let miner_v9_cid = new_manifest
            .builtin_actor_codes()
            .find(|cid| is_v9_miner_cid(cid))
            .context("Failed to retrieve miner v9 cid")?;

        self.add_migrator(
            *miner_v8_cid,
            miner::miner_migrator(*miner_v9_cid, &store, market_v8_state.proposals)?,
        );

        // self.add_migrator(prior_cid, market::market_migrator(market_v8_state)?);

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
    DB: 'static + Blockstore + Clone + Send + Sync,
{
    let new_manifest_cid = chain_config
        .height_infos
        .get(Height::Shark as usize)
        .ok_or_else(|| anyhow!("no height info for network version NV17"))?
        .bundle
        .as_ref()
        .ok_or_else(|| anyhow!("no bundle info for network version NV17"))?
        .manifest;

    blockstore.get(&new_manifest_cid)?.ok_or_else(|| {
        anyhow!(
            "manifest for network version NV17 not found in blockstore: {}",
            new_manifest_cid
        )
    })?;

    // Add migration specification verification
    let verifier = Arc::new(Verifier::default());

    // Add post-migration steps
    let post_migration_actions = [verifreg_market::create_verifreg_market_actor]
        .into_iter()
        .map(|action| Arc::new(action) as PostMigrationAction<DB>)
        .collect();
    // let post_migration_actions = Vec::new();

    let mut migration = StateMigration::<DB>::new(Some(verifier), post_migration_actions);
    migration.add_nv17_migrations(blockstore.clone(), state, &new_manifest_cid)?;

    let mut actors_in = StateTree::new_from_root(blockstore.clone(), state)?;

    // Sets empty datacap actor to migrate from
    let (version, new_manifest_data): (u32, Cid) = blockstore
        .get_cbor(&new_manifest_cid)?
        .ok_or_else(|| anyhow!("new manifest not found"))?;
    let new_manifest = ManifestNew::load(&blockstore, &new_manifest_data, version)?;
    let datacap_code = new_manifest
        .code_by_id(fil_actors_shared::v9::builtin::DATACAP_TOKEN_ACTOR_ID as _)
        .context("datacap code not found in new manifest")?;
    actors_in.set_actor(
        &Address::new_id(fil_actors_shared::v9::builtin::DATACAP_TOKEN_ACTOR_ID),
        ActorState::new_empty(*datacap_code, None),
    )?;

    let actors_out = StateTree::new(blockstore.clone(), StateTreeVersion::V5)?;
    let new_state =
        migration.migrate_state_tree(blockstore.clone(), epoch, actors_in, actors_out)?;

    Ok(new_state)
}
