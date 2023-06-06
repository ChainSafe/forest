// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! This module contains the migration logic for the `NV17` upgrade for the verifreg and market
//! actor.

use forest_shim::{deal::DealID, state_tree::StateTree};
use fvm_ipld_blockstore::Blockstore;

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
        // FIXME: `DEFAULT_BIT_WIDTH` on rust side is 3 while it's 5 on go side. Revisit to make sure
        // it does not effect `load` API here. (Go API takes bit_width=5 for loading while Rust API does not)
        //
        // P.S. Because of lifetime limitation, this is not stored as a field of `MinerMigrator` like in Go code
        let market_proposals = fil_actors_shared::v8::Array::<
            fil_actor_market_state::v8::DealProposal,
            _,
        >::load(&self.market_state_v8.proposals, &store)?;

        let next_allocation_id: fil_actor_verifreg_state::v9::AllocationID = 1;

        todo!()
    }
}
