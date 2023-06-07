// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! This module contains the migration logic for the `NV17` upgrade for the verifreg and market
//! actor.

use anyhow::Context;
use forest_shim::{deal::DealID, state_tree::StateTree};
use forest_utils::db::CborStoreExt;
use fvm_ipld_blockstore::Blockstore;
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

impl<BS: Blockstore> PostMigrator<BS> for VerifregMarketPostMigrator {
    fn post_migrate_state(&self, store: &BS, actors_out: &mut StateTree<BS>) -> anyhow::Result<()> {
        const HAMT_BIT_WIDTH: u32 = fil_actors_shared::v9::builtin::HAMT_BIT_WIDTH;

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

        todo!()
    }
}
