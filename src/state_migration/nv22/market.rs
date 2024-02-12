// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! This module contains the migration logic for the `NV22` upgrade for the
//! Market actor.
use std::sync::Arc;

use crate::shim::econ::TokenAmount;
use crate::{
    shim::address::Address, state_migration::common::MigrationCache, utils::db::CborStoreExt,
};
use anyhow::Context;
use cid::{multibase::Base, Cid};
use fil_actor_market_state::v12::{DealState as DealStateOld, State as MarketStateOld};
use fil_actor_market_state::v13::{
    DealState as DealStateNew, ProviderSectorsMap, State as MarketStateNew, PROVIDER_SECTORS_CONFIG,
};
use fil_actors_shared::fvm_ipld_amt;
use fil_actors_shared::v12::{runtime::Policy as PolicyOld, Array as ArrayOld, Map as MapOld};
use fil_actors_shared::v13::{runtime::Policy as PolicyNew, Array as ArrayNew};
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::CborStore;
use fvm_shared4::sector::SectorID;

use crate::state_migration::common::{
    ActorMigration, ActorMigrationInput, ActorMigrationOutput, TypeMigration, TypeMigrator,
};

use super::miner::ProviderSectors;

pub struct MarketMigrator {
    provider_sectors: Arc<ProviderSectors>,
    policy_new: PolicyNew,
    out_cid: Cid,
}
pub(in crate::state_migration) fn market_migrator<BS: Blockstore>(
    provider_sectors: Arc<ProviderSectors>,
    policy_old: &PolicyOld,
    policy_new: &PolicyNew,
    store: &Arc<BS>,
    out_cid: Cid,
) -> anyhow::Result<Arc<dyn ActorMigration<BS> + Send + Sync>> {
    Ok(Arc::new(MarketMigrator {
        provider_sectors,
        policy_new: policy_new.clone(),
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

        let (provider_sectors, new_states) =
            self.migrate_provider_sectors_and_states::<BS>(store, input, &in_state.states)?;

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
    ) -> anyhow::Result<(Cid, Cid)> {
        let prev_in_states = input
            .cache
            .get(&market_prev_deal_states_in_key(&input.address));

        let prev_out_states = input
            .cache
            .get(&market_prev_deal_states_out_key(&input.address));

        let prev_out_provider_sectors = input
            .cache
            .get(&market_prev_provider_sectors_out_key(&input.address));

        let (provider_sectors_root, new_state_array_root) =
            if let (Some(prev_in_states), Some(prev_out_states), Some(prev_out_provider_sectors)) =
                (prev_in_states, prev_out_states, prev_out_provider_sectors)
            {
                self.migrate_provider_sectors_and_states_with_diff::<BS>(
                    store,
                    &prev_in_states,
                    &prev_out_states,
                    &prev_out_provider_sectors,
                    states,
                )?
            } else {
                self.migrate_provider_sectors_and_states_with_scratch(store, &input, states)?
            };

        input
            .cache
            .insert(market_prev_deal_states_in_key(&input.address), *states);

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
        prev_out_states_cid: &Cid,
        prev_out_provider_sectors_cid: &Cid,
        in_states_cid: &Cid,
    ) -> anyhow::Result<(Cid, Cid)> {
        //let prev_in_states: MarketStateOld = store
        //    .get_cbor(prev_in_states_cid)?
        //    .context("failed to load prev_in_states")?;

        let prev_in_states = ArrayOld::<DealStateOld, _>::load(&prev_in_states_cid, store)?;
        let in_states = ArrayOld::<DealStateOld, _>::load(&in_states_cid, store)?;

        let mut prev_out_states = ArrayOld::<DealStateNew, _>::load(&prev_out_states_cid, store)?;

        let prev_out_provider_sectors = ProviderSectorsMap::load(
            store,
            prev_out_provider_sectors_cid,
            PROVIDER_SECTORS_CONFIG,
            "provider sectors",
        )?;

        let add_provider_sector_entry = |deal| -> anyhow::Result<u64> {
            let deal_to_sector = self.provider_sectors.deal_to_sector.read();
            let sector_id = deal_to_sector
                .get(&deal)
                .context(format!("deal {deal} not found in provider sectors"))?;

            Ok(sector_id.number)
        };
        let diffs = fvm_ipld_amt::diff(&prev_in_states, &in_states)?;
        for change in diffs {
            let deal = change.key;

            use fvm_ipld_amt::ChangeType::*;
            match change.change_type() {
                Add => {
                    let old_state = change.after.context("missing after state")?;

                    let sector_number = if old_state.sector_start_epoch != -1 {
                        add_provider_sector_entry(deal)?
                    } else {
                        0
                    };
                    let new_state = DealStateNew {
                        sector_number,
                        sector_start_epoch: old_state.sector_start_epoch,
                        last_updated_epoch: old_state.last_updated_epoch,
                        slash_epoch: old_state.slash_epoch,
                    };

                    prev_out_states.set(deal, new_state)?;
                }
                Remove => todo!(),
                Modify => todo!(),
            }
        }
        todo!()
    }

    fn migrate_provider_sectors_and_states_with_scratch(
        &self,
        store: &impl Blockstore,
        input: &ActorMigrationInput,
        states: &Cid,
    ) -> anyhow::Result<(Cid, Cid)> {
        todo!()
    }
}

// TODO make them global and available from all network migrations
fn miner_prev_sectors_in_key(addr: &Address) -> String {
    format!("prev_sectors_in_{addr}")
}

fn miner_prev_sectors_out_key(addr: &Address) -> String {
    format!("prev_sectors_out_{addr}")
}

fn market_prev_deal_states_in_key(addr: &Address) -> String {
    format!("prev_deal_states_in_{addr}")
}

fn market_prev_deal_states_out_key(addr: &Address) -> String {
    format!("prev_deal_states_out_{addr}")
}

fn market_prev_provider_sectors_out_key(addr: &Address) -> String {
    format!("prev_provider_sectors_out_{addr}")
}
