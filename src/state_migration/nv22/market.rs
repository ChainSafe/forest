// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! This module contains the migration logic for the `NV22` upgrade for the
//! Market actor.
use std::sync::Arc;

use crate::shim::econ::TokenAmount;
use crate::{shim::address::Address, utils::db::CborStoreExt};
use ahash::HashMap;
use anyhow::Context;
use cid::Cid;
use fil_actor_market_state::v12::{
    DealArray as DealArrayOld, DealState as DealStateOld, State as MarketStateOld,
};
use fil_actor_market_state::v13::{
    DealState as DealStateNew, ProviderSectorsMap as ProviderSectorsMapNew, SectorDealIDs,
    SectorDealsMap, State as MarketStateNew, PROVIDER_SECTORS_CONFIG, SECTOR_DEALS_CONFIG,
};
use fil_actors_shared::fvm_ipld_amt;
use fil_actors_shared::v12::Array as ArrayOld;
use fil_actors_shared::v13::Array as ArrayNew;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::CborStore;
use fvm_shared4::deal::DealID;
use fvm_shared4::sector::SectorNumber;
use fvm_shared4::ActorID;

use crate::state_migration::common::{ActorMigration, ActorMigrationInput, ActorMigrationOutput};

use super::miner::ProviderSectors;

pub struct MarketMigrator {
    provider_sectors: Arc<ProviderSectors>,
    out_cid: Cid,
}
pub(in crate::state_migration) fn market_migrator<BS: Blockstore>(
    provider_sectors: Arc<ProviderSectors>,
    out_cid: Cid,
) -> anyhow::Result<Arc<dyn ActorMigration<BS> + Send + Sync>> {
    Ok(Arc::new(MarketMigrator {
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
        let in_state: MarketStateOld = store
            .get_cbor(&input.head)?
            .context("failed to load state")?;

        let (provider_sectors, new_states) = self.migrate_provider_sectors_and_states::<BS>(
            store,
            input,
            &in_state.states,
            &in_state.proposals,
        )?;

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
}

impl MarketMigrator {
    fn migrate_provider_sectors_and_states<BS: Blockstore>(
        &self,
        store: &impl Blockstore,
        input: ActorMigrationInput,
        states: &Cid,
        proposals: &Cid,
    ) -> anyhow::Result<(Cid, Cid)> {
        let prev_in_states = input
            .cache
            .get(&market_prev_deal_states_in_key(&input.address));

        let prev_in_proposals = input
            .cache
            .get(&market_prev_deal_proposals_in_key(&input.address));

        let prev_out_states = input
            .cache
            .get(&market_prev_deal_states_out_key(&input.address));

        let prev_out_provider_sectors = input
            .cache
            .get(&market_prev_provider_sectors_out_key(&input.address));

        let (provider_sectors_root, new_state_array_root) = if let (
            Some(prev_in_states),
            Some(prev_in_proposals),
            Some(prev_out_states),
            Some(prev_out_provider_sectors),
        ) = (
            prev_in_states,
            prev_in_proposals,
            prev_out_states,
            prev_out_provider_sectors,
        ) {
            self.migrate_provider_sectors_and_states_with_diff::<BS>(
                store,
                &prev_in_states,
                &prev_in_proposals,
                &prev_out_states,
                &prev_out_provider_sectors,
                states,
            )?
        } else {
            self.migrate_provider_sectors_and_states_with_scratch(store, states)?
        };

        input
            .cache
            .insert(market_prev_deal_states_in_key(&input.address), *states);

        input.cache.insert(
            market_prev_deal_proposals_in_key(&input.address),
            *proposals,
        );

        input.cache.insert(
            market_prev_deal_states_out_key(&input.address),
            new_state_array_root,
        );

        input.cache.insert(
            market_prev_provider_sectors_out_key(&input.address),
            provider_sectors_root,
        );

        Ok((provider_sectors_root, new_state_array_root))
    }

    fn migrate_provider_sectors_and_states_with_diff<BS: Blockstore>(
        &self,
        store: &impl Blockstore,
        prev_in_states_cid: &Cid,
        prev_in_proposals_cid: &Cid,
        prev_out_states_cid: &Cid,
        prev_out_provider_sectors_cid: &Cid,
        in_states_cid: &Cid,
    ) -> anyhow::Result<(Cid, Cid)> {
        let prev_in_states = ArrayOld::<DealStateOld, _>::load(prev_in_states_cid, store)?;
        let in_states = ArrayOld::<DealStateOld, _>::load(in_states_cid, store)?;

        let mut prev_out_states = ArrayOld::<DealStateNew, _>::load(prev_out_states_cid, store)?;

        let mut prev_out_provider_sectors = ProviderSectorsMapNew::load(
            store,
            prev_out_provider_sectors_cid,
            PROVIDER_SECTORS_CONFIG,
            "provider sectors",
        )?;

        let proposals_array = DealArrayOld::load(prev_in_proposals_cid, store)?;

        // changesets to be applied to `prev_out_provider_sectors`
        let mut provider_sectors: HashMap<ActorID, HashMap<SectorNumber, Vec<DealID>>> =
            HashMap::default();
        let mut provider_sectors_remove: HashMap<ActorID, HashMap<SectorNumber, Vec<DealID>>> =
            HashMap::default();

        let mut add_provider_sector_entry = |deal| -> anyhow::Result<u64> {
            let deal_to_sector = self.provider_sectors.deal_to_sector.read();
            let sector_id = deal_to_sector
                .get(&deal)
                .context(format!("deal {deal} not found in provider sectors"))?;

            provider_sectors
                .entry(sector_id.miner)
                .or_default()
                .entry(sector_id.number)
                .or_default()
                .push(deal);

            Ok(sector_id.number)
        };

        let mut remove_provider_sector_entry =
            |deal, mut new_state_prev_state: DealStateNew| -> anyhow::Result<DealStateNew> {
                let sector_number = new_state_prev_state.sector_number;
                let proposal = proposals_array.get(deal)?.context("proposal not found")?;

                let provider_id = proposal.provider.id()? as ActorID;
                new_state_prev_state.sector_number = 0;
                provider_sectors_remove
                    .entry(provider_id)
                    .or_default()
                    .entry(sector_number)
                    .or_default()
                    .push(deal);

                Ok(new_state_prev_state)
            };

        let diffs = fvm_ipld_amt::diff(&prev_in_states, &in_states)?;
        for change in diffs {
            let deal = change.key;

            use fvm_ipld_amt::ChangeType::*;
            match change.change_type() {
                Add => {
                    let old_state = change.after.context("missing after state")?;
                    let sector_number = if old_state.slash_epoch != -1 {
                        add_provider_sector_entry(deal)?
                    } else {
                        0
                    };
                    let new_state = DealStateNew {
                        sector_number,
                        last_updated_epoch: old_state.last_updated_epoch,
                        sector_start_epoch: old_state.sector_start_epoch,
                        slash_epoch: old_state.slash_epoch,
                    };

                    prev_out_states.set(deal, new_state)?;
                }
                Remove => {
                    let prev_out_state = prev_out_states.get(deal)?.context("deal not found")?;
                    if prev_out_state.slash_epoch != 1 {
                        // Comment from Go implementation:
                        // > if the previous OUT state was not slashed then it has a provider sector entry that needs to be removed
                        remove_provider_sector_entry(deal, *prev_out_state)?;
                    }

                    prev_out_states.delete(deal)?;
                }
                Modify => {
                    let prev_old_state = change.before.context("missing before state")?;
                    let old_state = change.after.context("missing after state")?;

                    let mut new_state = *prev_out_states.get(deal)?.context("deal not found")?;
                    new_state.slash_epoch = old_state.slash_epoch;
                    new_state.last_updated_epoch = old_state.last_updated_epoch;
                    new_state.sector_start_epoch = old_state.sector_start_epoch;

                    let new_state =
                        if prev_old_state.slash_epoch != -1 && old_state.slash_epoch != -1 {
                            remove_provider_sector_entry(deal, new_state)?
                        } else {
                            new_state
                        };

                    prev_out_states.set(deal, new_state)?;
                }
            }
        }

        // process prevOutProviderSectors, first removes, then adds

        for (provider_id, sectors) in provider_sectors_remove.iter() {
            let actor_sectors = prev_out_provider_sectors.get(provider_id)?;

            // From Go implementation:
            // > this is fine, all sectors of this miner were already not present
            // > in ProviderSectors. Sadly because the default value of a non-present
            // > sector number in deal state is 0, we can't tell if a sector was
            // > removed or if it was never there to begin with, which is why we
            // > may occasionally end up here.
            let actor_sectors = if let Some(actor_sectors) = actor_sectors {
                actor_sectors
            } else {
                continue;
            };

            let mut actor_sectors =
                SectorDealsMap::load(store, actor_sectors, SECTOR_DEALS_CONFIG, "sector deals")?;

            for (sector, deals) in sectors.iter() {
                let sector_deals = actor_sectors.get(sector)?;
                let sector_deals = if let Some(sector_deals) = sector_deals {
                    sector_deals
                } else {
                    continue;
                };

                let mut sector_deals = sector_deals.clone();
                for deal in deals.iter() {
                    if let Some(idx) = sector_deals.deals.iter().position(|d| d == deal) {
                        sector_deals.deals.remove(idx);
                    }
                }

                if sector_deals.deals.is_empty() {
                    actor_sectors.delete(sector)?;
                } else {
                    actor_sectors.set(sector, sector_deals)?;
                }
            }

            if !actor_sectors.is_empty() {
                let new_actor_sectors_root = actor_sectors.flush()?;
                prev_out_provider_sectors.set(provider_id, new_actor_sectors_root)?;
            } else {
                prev_out_provider_sectors.delete(provider_id)?;
            }
        }

        for (provider_id, sectors) in provider_sectors.iter() {
            let actor_sectors_root = prev_out_provider_sectors.get(provider_id)?;
            let mut actor_sectors = if let Some(actor_sectors_root) = actor_sectors_root {
                SectorDealsMap::load(
                    store,
                    actor_sectors_root,
                    SECTOR_DEALS_CONFIG,
                    "sector deals",
                )?
            } else {
                SectorDealsMap::empty(store, SECTOR_DEALS_CONFIG, "sector deals")
            };

            for (sector, deals) in sectors.iter() {
                actor_sectors.set(
                    sector,
                    SectorDealIDs {
                        deals: deals.clone(),
                    },
                )?;
            }

            let new_actor_sectors_root = actor_sectors.flush()?;
            prev_out_provider_sectors.set(provider_id, new_actor_sectors_root)?;
        }

        let out_provider_sectors_root = prev_out_provider_sectors.flush()?;
        let out_states = prev_out_states.flush()?;

        Ok((out_provider_sectors_root, out_states))
    }

    fn migrate_provider_sectors_and_states_with_scratch(
        &self,
        store: &impl Blockstore,
        states: &Cid,
    ) -> anyhow::Result<(Cid, Cid)> {
        let old_state_array = ArrayOld::<DealStateOld, _>::load(states, store)?;
        let mut new_state_array = ArrayNew::<DealStateNew, _>::new(store);

        let mut provider_sectors: HashMap<ActorID, HashMap<SectorNumber, Vec<DealID>>> =
            HashMap::default();

        // https://github.com/filecoin-project/FIPs/blob/master/FIPS/fip-0076.md#migration
        // FIP-0076: For each deal state object in the market actor state that has a terminated epoch set to -1
        old_state_array.for_each(|deal_id, old_state| {
            let sector_number = if old_state.slash_epoch == -1 {
                // find the corresponding deal proposal object and extract the provider's actor ID;
                if let Some(sector_id) = self.provider_sectors.deal_to_sector.read().get(&deal_id) {
                    provider_sectors
                        .entry(sector_id.miner)
                        .or_default()
                        .entry(sector_id.number)
                        .or_default()
                        .push(deal_id);
                    // set the new deal state object's sector number to the sector ID found
                    sector_id.number
                } else {
                    0
                }
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

        for (miner, sectors) in provider_sectors.iter() {
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

fn market_prev_deal_states_in_key(addr: &Address) -> String {
    format!("prev_deal_states_in_{addr}")
}

fn market_prev_deal_proposals_in_key(addr: &Address) -> String {
    format!("prev_deal_proposals_in_{addr}")
}

fn market_prev_deal_states_out_key(addr: &Address) -> String {
    format!("prev_deal_states_out_{addr}")
}

fn market_prev_provider_sectors_out_key(addr: &Address) -> String {
    format!("prev_provider_sectors_out_{addr}")
}
