// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! This module contains the migration logic for the `NV17` upgrade for the verifreg and market
//! actor.

use std::sync::Arc;

use cid::Cid;
use forest_shim::state_tree::StateTree;
use fvm_ipld_blockstore::Blockstore;

use crate::common::{ActorMigration, ActorMigrationInput, ActorMigrationOutput, PostMigrator};

pub(super) struct VerifregMarketPostMigrator {
    pub market_v8_state: fil_actor_market_state::v8::State,
}

impl<BS: Blockstore> PostMigrator<BS> for VerifregMarketPostMigrator {
    fn post_migrate_state(&self, store: &BS, actors_out: &mut StateTree<BS>) -> anyhow::Result<()> {
        todo!()
    }
}
