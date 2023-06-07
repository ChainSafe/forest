// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! This module contains the migration logic for the `NV17` upgrade for the verifreg and market
//! actor.

use anyhow::Context;
use forest_shim::{
    deal::DealID,
    machine::ManifestV3,
    state_tree::{ActorState, StateTree},
};
use forest_utils::db::CborStoreExt;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::CborStore;
use fvm_ipld_hamt::BytesKey;
use fvm_shared::address::Address;

use super::ChainEpoch;
use crate::common::{ActorMigration, ActorMigrationInput, ActorMigrationOutput, PostMigrator};

pub(super) struct VerifregMarketPostMigrator {
    pub prior_epoch: ChainEpoch,
    pub init_state_v8: fil_actor_init_state::v8::State,
    pub market_state_v8: fil_actor_market_state::v8::State,
    pub verifreg_state_v8: fil_actor_verifreg_state::v8::State,
    pub pending_verified_deals: Vec<DealID>,
}

impl<BS: Blockstore + Clone> PostMigrator<BS> for VerifregMarketPostMigrator {
    fn post_migrate_state(&self, store: &BS, actors_out: &mut StateTree<BS>) -> anyhow::Result<()> {
        const HAMT_BIT_WIDTH: u32 = fil_actors_shared::v9::builtin::HAMT_BIT_WIDTH;

        // `migrateVerifreg`

        // FIXME: `DEFAULT_BIT_WIDTH` on rust side is 3 while it's 5 on go side. Revisit to make sure
        // it does not effect `load` API here. (Go API takes bit_width=5 for loading while Rust API does not)
        //
        // P.S. Because of lifetime limitation, this is not stored as a field of `MinerMigrator` like in Go code
        let market_proposals = fil_actors_shared::v8::Array::<
            fil_actor_market_state::v8::DealProposal,
            _,
        >::load(&self.market_state_v8.proposals, &store)?;

        let mut next_allocation_id: fil_actor_verifreg_state::v9::AllocationID = 1;
        let mut allocations_map_map = fil_actors_shared::v9::MapMap::<
            BS,
            fil_actor_verifreg_state::v9::Allocation,
            Address,
            fil_actor_verifreg_state::v9::AllocationID,
        >::new(store, HAMT_BIT_WIDTH, HAMT_BIT_WIDTH);
        let mut deal_allocation_tuples = vec![];

        for &deal_id in &self.pending_verified_deals {
            let proposal = market_proposals
                .get(deal_id)?
                .context("Failed to get pending deal proposal")?;
            let client_id_address = self
                .init_state_v8
                .resolve_address(store, &proposal.client)?
                .context("Failed to find client in init actor map")?;
            let provider_id_address = self
                .init_state_v8
                .resolve_address(store, &proposal.provider)?
                .context("Failed to find provider in init actor map")?;

            let mut expiration = fil_actors_shared::v9::runtime::policy_constants::MAXIMUM_VERIFIED_ALLOCATION_EXPIRATION + self.prior_epoch;
            if expiration > proposal.start_epoch {
                expiration = proposal.start_epoch;
            }

            allocations_map_map.put(
                client_id_address,
                next_allocation_id,
                fil_actor_verifreg_state::v9::Allocation {
                    client: client_id_address.id()?,
                    provider: provider_id_address.id()?,
                    data: proposal.piece_cid,
                    size: proposal.piece_size,
                    term_min: proposal.duration(),
                    term_max: fil_actors_shared::v9::runtime::policy_constants::MAX_SECTOR_EXPIRATION_EXTENSION,
                    expiration: expiration,
                },
            )?;

            deal_allocation_tuples.push((deal_id, next_allocation_id));

            next_allocation_id += 1;
        }

        let mut empty_map = fil_actors_shared::v9::make_empty_map::<_, ()>(store, HAMT_BIT_WIDTH);
        let verifreg_state_v9 = fil_actor_verifreg_state::v9::State {
            root_key: self.verifreg_state_v8.root_key,
            verifiers: self.verifreg_state_v8.verifiers,
            remove_data_cap_proposal_ids: self.verifreg_state_v8.remove_data_cap_proposal_ids,
            allocations: allocations_map_map.flush()?,
            next_allocation_id: next_allocation_id,
            claims: empty_map.flush()?,
        };
        let verifreg_head = store.put_cbor_default(&verifreg_state_v9)?;

        // `migrateMarket`
        let mut pending_deal_allocation_id_map =
            fil_actors_shared::v9::make_empty_map::<_, u64>(store, HAMT_BIT_WIDTH);
        for (deal_id, allocation_id) in deal_allocation_tuples {
            pending_deal_allocation_id_map
                .set(fil_actors_shared::v9::u64_key(deal_id), allocation_id)?;
        }
        let pending_deal_allocation_id_map_root = pending_deal_allocation_id_map.flush()?;
        let deal_states_v8 = fil_actors_shared::v8::Array::<
            fil_actor_market_state::v8::DealState,
            _,
        >::load(&self.market_state_v8.states, store)?;
        // TODO: Make sure bitwidth is correct with this API
        let mut deal_states_v9 = fil_actors_shared::v9::Array::<
            fil_actor_market_state::v9::DealState,
            _,
        >::new_with_bit_width(
            store, fil_actor_market_state::v9::STATES_AMT_BITWIDTH
        );
        deal_states_v8.for_each(|key, state| {
            deal_states_v9.set(
                key,
                fil_actor_market_state::v9::DealState {
                    sector_start_epoch: state.sector_start_epoch,
                    last_updated_epoch: state.last_updated_epoch,
                    slash_epoch: state.slash_epoch,
                    // `NO_ALLOCATION_ID` is not available under `v9` but the value is correct.
                    verified_claim: fil_actor_market_state::v10::NO_ALLOCATION_ID,
                },
            )?;

            Ok(())
        })?;

        let market_state_v9 = fil_actor_market_state::v9::State {
            proposals: self.market_state_v8.proposals,
            states: deal_states_v9.flush()?,
            pending_proposals: self.market_state_v8.pending_proposals,
            escrow_table: self.market_state_v8.escrow_table,
            locked_table: self.market_state_v8.locked_table,
            next_id: self.market_state_v8.next_id,
            deal_ops_by_epoch: self.market_state_v8.deal_ops_by_epoch,
            last_cron: self.market_state_v8.last_cron,
            total_client_locked_collateral: self
                .market_state_v8
                .total_client_locked_collateral
                .clone(),
            total_provider_locked_collateral: self
                .market_state_v8
                .total_provider_locked_collateral
                .clone(),
            total_client_storage_fee: self.market_state_v8.total_client_storage_fee.clone(),
            pending_deal_allocation_ids: pending_deal_allocation_id_map_root,
        };
        let market_head = store.put_cbor_default(&market_state_v9)?;

        let sys_actor = actors_out
            .get_actor(&forest_shim::address::Address::SYSTEM_ACTOR)?
            .ok_or_else(|| anyhow::anyhow!("Couldn't get sys actor state"))?;
        let sys_state: super::SystemStateNew = store
            .get_cbor(&sys_actor.state)?
            .ok_or_else(|| anyhow::anyhow!("Couldn't get state v9"))?;

        let manifest = ManifestV3::load(&store, &sys_state.builtin_actors, 1)?;

        // TODO: Need API(s) for getting the missing actor codes
        // let verifreg_actor = ActorState::new(manifest. , state, balance, sequence, address)

        todo!()
    }
}
