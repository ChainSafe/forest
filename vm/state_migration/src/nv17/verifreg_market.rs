// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! This module contains the migration logic for the `NV17` upgrade for the verifreg and market
//! actor.

use std::sync::Arc;

use cid::Cid;
use forest_shim::state_tree::StateTree;
use fvm_ipld_blockstore::Blockstore;

use crate::common::{ActorMigration, ActorMigrationInput, ActorMigrationOutput};

/// Creates the Ethereum Account Manager actor in the state tree.
pub fn create_verifreg_market_actor<BS: Blockstore + Clone + Send + Sync>(
    store: &BS,
    actors_out: &mut StateTree<BS>,
) -> anyhow::Result<()> {
    todo!()
}
