// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! This module contains the migration logic for the `NV17` upgrade for the `verifreg` and `market`
//! actor.

use crate::shim::{
    address::Address,
    deal::DealID,
    state_tree::{ActorState, StateTree},
};
use crate::utils::db::CborStoreExt;
use ahash::HashMap;
use anyhow::Context;
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_hamt::BytesKey;

use super::super::common::PostMigrator;
use super::super::ChainEpoch;

pub(super) struct VerifregMarketPostMigrator {
    pub prior_epoch: ChainEpoch,
    pub init_state_v8: fil_actor_init_state::v8::State,
    pub market_state_v8: fil_actor_market_state::v8::State,
    pub verifreg_state_v8: fil_actor_verifreg_state::v8::State,
    pub pending_verified_deals: Vec<DealID>,
    pub verifreg_actor_v8: ActorState,
    pub market_actor_v8: ActorState,
    pub verifreg_code: Cid,
    pub market_code: Cid,
}

impl<BS: Blockstore + Clone> PostMigrator<BS> for VerifregMarketPostMigrator {
    fn post_migrate_state(&self, store: &BS, actors_out: &mut StateTree<BS>) -> anyhow::Result<()> {
        use fil_actors_shared::v9::builtin::HAMT_BIT_WIDTH;

        // `migrateVerifreg`

        // Because of lifetime limitation, this is not stored as a field of `MinerMigrator` like in Go code
        let market_proposals = fil_actors_shared::v8::Array::<
            fil_actor_market_state::v8::DealProposal,
            _,
        >::load(&self.market_state_v8.proposals, store)?;

        let mut next_allocation_id: fil_actor_verifreg_state::v9::AllocationID = 1;
        let mut allocations_map_map = HashMap::default();
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

            let allocation = fil_actor_verifreg_state::v9::Allocation {
                client: client_id_address.id()?,
                provider: provider_id_address.id()?,
                data: proposal.piece_cid,
                size: proposal.piece_size,
                term_min: proposal.duration(),
                term_max: fil_actors_shared::v9::runtime::policy_constants::MAX_SECTOR_EXPIRATION_EXTENSION,
                expiration,
            };

            let entry = allocations_map_map
                .entry(client_id_address)
                .or_insert_with(|| {
                    fil_actors_shared::v9::make_empty_map::<
                        _,
                        fil_actor_verifreg_state::v9::Allocation,
                    >(store, HAMT_BIT_WIDTH)
                });
            entry.set(
                fil_actors_shared::v8::u64_key(next_allocation_id),
                allocation,
            )?;

            deal_allocation_tuples.push((deal_id, next_allocation_id));

            next_allocation_id += 1;
        }
        let mut allocations_map = fil_actors_shared::v9::make_empty_map(store, HAMT_BIT_WIDTH);
        for (client_id, mut client_allocations_map) in allocations_map_map {
            let client_allocations_map_cid = client_allocations_map.flush()?;
            allocations_map.set(
                // Note: `client_id.payload_bytes()` produces different output than `client_id.payload().to_bytes()`
                // see <https://github.com/ChainSafe/fil-actor-states/issues/150>
                BytesKey(client_id.payload_bytes()),
                client_allocations_map_cid,
            )?;
        }

        let mut empty_map = fil_actors_shared::v9::make_empty_map::<BS, ()>(store, HAMT_BIT_WIDTH);
        let verifreg_state_v9 = fil_actor_verifreg_state::v9::State {
            root_key: self.verifreg_state_v8.root_key,
            verifiers: self.verifreg_state_v8.verifiers,
            remove_data_cap_proposal_ids: self.verifreg_state_v8.remove_data_cap_proposal_ids,
            allocations: allocations_map.flush()?,
            next_allocation_id,
            claims: empty_map.flush()?,
        };

        let verifreg_head = store.put_cbor_default(&verifreg_state_v9)?;

        // `migrateMarket`
        let mut pending_deal_allocation_id_map =
            fil_actors_shared::v9::make_empty_map::<BS, i64>(store, HAMT_BIT_WIDTH);
        for (deal_id, allocation_id) in deal_allocation_tuples {
            pending_deal_allocation_id_map
                .set(fil_actors_shared::v9::u64_key(deal_id), allocation_id as _)?;
        }
        let pending_deal_allocation_id_map_root = pending_deal_allocation_id_map.flush()?;
        let deal_states_v8 = fil_actors_shared::v8::Array::<
            fil_actor_market_state::v8::DealState,
            _,
        >::load(&self.market_state_v8.states, store)?;

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

        let verifreg_actor = ActorState::new(
            self.verifreg_code,
            verifreg_head,
            self.verifreg_actor_v8.balance.clone().into(),
            self.verifreg_actor_v8.sequence,
            None, // ActorV4 contains no delegated address
        );

        actors_out.set_actor(&Address::VERIFIED_REGISTRY_ACTOR, verifreg_actor)?;

        let market_actor = ActorState::new(
            self.market_code,
            market_head,
            self.market_actor_v8.balance.clone().into(),
            self.market_actor_v8.sequence,
            None, // ActorV4 contains no delegated address
        );

        actors_out.set_actor(&Address::MARKET_ACTOR, market_actor)?;

        Ok(())
    }
}
