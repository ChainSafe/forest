// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_shim::{
    address::Address,
    machine::Manifest,
    state_tree::{ActorState, StateTree},
};
use forest_utils::db::BlockstoreExt;
use fvm_ipld_blockstore::Blockstore;

/// Creates the Ethereum Account Manager actor in the state tree.
pub fn create_market_actor<BS: Blockstore + Clone + Send + Sync>(
    store: &BS,
    actors_out: &mut StateTree<BS>,
) -> anyhow::Result<()> {
    Ok(())
}
