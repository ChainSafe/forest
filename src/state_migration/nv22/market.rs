// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! This module contains the migration logic for the `NV22` upgrade for the
//! Market actor.
use std::sync::Arc;

use crate::shim::econ::TokenAmount;
use crate::utils::db::CborStoreExt;
use anyhow::Context as _;
use cid::Cid;
use fil_actor_market_state::v12::{
    DealProposal, DealState as DealStateOld, State as MarketStateOld,
};
use fil_actor_market_state::v13::{
    DealState as DealStateNew, PROVIDER_SECTORS_CONFIG,
    ProviderSectorsMap as ProviderSectorsMapNew, SECTOR_DEALS_CONFIG, STATES_AMT_BITWIDTH,
    SectorDealIDs, SectorDealsMap, State as MarketStateNew,
};

use fil_actors_shared::v12::Array as ArrayOld;
use fil_actors_shared::v13::Array as ArrayNew;
use fvm_ipld_blockstore::Blockstore;
use fvm_shared4::clock::ChainEpoch;

use crate::state_migration::common::{ActorMigration, ActorMigrationInput, ActorMigrationOutput};

use super::miner::ProviderSectors;

pub struct MarketMigrator {
    upgrade_epoch: ChainEpoch,
    provider_sectors: Arc<ProviderSectors>,
    out_cid: Cid,
}
pub(in crate::state_migration) fn market_migrator<BS: Blockstore>(
    upgrade_epoch: ChainEpoch,
    provider_sectors: Arc<ProviderSectors>,
    out_cid: Cid,
) -> anyhow::Result<Arc<dyn ActorMigration<BS> + Send + Sync>> {
    Ok(Arc::new(MarketMigrator {
        upgrade_epoch,
        provider_sectors,
        out_cid,
    }))
}

impl<BS: Blockstore> ActorMigration<BS> for MarketMigrator {
    fn migrate_state(
        &self,
        store: &BS,
        input: ActorMigrationInput,
    ) -> anyhow::Result<Option<ActorMigrationOutput>> {
        let in_state: MarketStateOld = store.get_cbor_required(&input.head)?;

        let (provider_sectors, new_states) =
            self.migrate_provider_sectors_and_states(store, &in_state.states, &in_state.proposals)?;

        let out_state = MarketStateNew {
            proposals: in_state.proposals,
            states: new_states,
            pending_proposals: in_state.pending_proposals,
            escrow_table: in_state.escrow_table,
            locked_table: in_state.locked_table,
            next_id: in_state.next_id,
            deal_ops_by_epoch: in_state.deal_ops_by_epoch,
            last_cron: in_state.last_cron,
            total_client_locked_collateral: TokenAmount::from(
                in_state.total_client_locked_collateral,
            )
            .into(),
            total_provider_locked_collateral: TokenAmount::from(
                in_state.total_provider_locked_collateral,
            )
            .into(),
            total_client_storage_fee: TokenAmount::from(in_state.total_client_storage_fee).into(),
            pending_deal_allocation_ids: in_state.pending_deal_allocation_ids,
            provider_sectors,
        };

        let new_head = store.put_cbor_default(&out_state)?;

        Ok(Some(ActorMigrationOutput {
            new_code_cid: self.out_cid,
            new_head,
        }))
    }

    fn is_deferred(&self) -> bool {
        true
    }
}

impl MarketMigrator {
    fn migrate_provider_sectors_and_states(
        &self,
        store: &impl Blockstore,
        states: &Cid,
        proposals: &Cid,
    ) -> anyhow::Result<(Cid, Cid)> {
        //dbg!("running market migration");
        let (provider_sectors_root, new_state_array_root) =
            self.migrate_provider_sectors_and_states_with_scratch(store, states, proposals)?;

        Ok((provider_sectors_root, new_state_array_root))
    }

    /// This method implements the migration logic as outlined in the [FIP-0076](https://github.com/filecoin-project/FIPs/blob/master/FIPS/fip-0076.md#migration)
    // > For each deal state object in the market actor state that has a terminated epoch set to -1:
    // > * find the corresponding deal proposal object and extract the provider's actor ID;
    // > * in the provider's miner state, find the ID of the sector with the corresponding deal ID in sector metadata;
    // >   * if such a sector cannot be found, assert that the deal's end epoch has passed and use sector ID 0 [1];
    // > * set the new deal state object's sector number to the sector ID found;
    // > * add the deal ID to the ProviderSectors mapping for the provider's actor ID and sector number.
    // > For each deal state object in the market actor state that has a terminated epoch set to any other value:
    // > * set the deal state object's sector number to 0.
    fn migrate_provider_sectors_and_states_with_scratch(
        &self,
        store: &impl Blockstore,
        states: &Cid,
        proposals: &Cid,
    ) -> anyhow::Result<(Cid, Cid)> {
        let old_state_array = ArrayOld::<DealStateOld, _>::load(states, store)?;
        let mut new_state_array =
            ArrayNew::<DealStateNew, _>::new_with_bit_width(store, STATES_AMT_BITWIDTH);

        let proposals_array = ArrayOld::<DealProposal, _>::load(proposals, store)?;

        old_state_array.for_each(|deal_id, old_state| {
            let proposal = proposals_array
                .get(deal_id)?
                .context("deal proposal not found")?;

            let sector_number =
                if old_state.slash_epoch == -1 && proposal.end_epoch >= self.upgrade_epoch {
                    // find the corresponding deal proposal object and extract the provider's actor ID;
                    self.provider_sectors
                        .deal_to_sector
                        .read()
                        .get(&deal_id)
                        .map(|sector_id| sector_id.number)
                        .unwrap_or(0)
                } else {
                    0
                };

            let new_state = DealStateNew {
                sector_number,
                last_updated_epoch: old_state.last_updated_epoch,
                sector_start_epoch: old_state.sector_start_epoch,
                slash_epoch: old_state.slash_epoch,
            };
            new_state_array.set(deal_id, new_state)?;

            Ok(())
        })?;

        let new_state_array_root = new_state_array.flush()?;
        let mut out_provider_sectors =
            ProviderSectorsMapNew::empty(store, PROVIDER_SECTORS_CONFIG, "provider sectors");

        for (miner, sectors) in self.provider_sectors.miner_to_sector_to_deals.read().iter() {
            let mut actor_sectors =
                SectorDealsMap::empty(store, SECTOR_DEALS_CONFIG, "sector deals");

            for (sector, deals) in sectors.iter() {
                actor_sectors.set(
                    sector,
                    SectorDealIDs {
                        deals: deals.clone(),
                    },
                )?;
            }

            out_provider_sectors.set(miner, actor_sectors.flush()?)?;
        }

        let out_provider_sectors_root = out_provider_sectors.flush()?;

        Ok((out_provider_sectors_root, new_state_array_root))
    }
}
