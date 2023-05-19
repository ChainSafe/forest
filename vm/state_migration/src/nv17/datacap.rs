// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

use cid::Cid;
use forest_shim::state_tree::StateTree;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_hamt::Hamt;

use super::DEFAULT_HAMT_BITWIDTH;
use crate::common::{
    ActorMigration, ActorMigrationInput, ActorMigrationOutput, TypeMigration, TypeMigrator,
};

pub struct DataCapMigrator(Cid);

pub(crate) fn datacap_migrator<BS: Blockstore + Clone + Send + Sync>(
    cid: Cid,
) -> Arc<dyn ActorMigration<BS> + Send + Sync> {
    Arc::new(DataCapMigrator(cid))
}

impl<BS: Blockstore + Clone + Send + Sync> ActorMigration<BS> for DataCapMigrator {
    fn migrate_state(
        &self,
        store: BS,
        input: ActorMigrationInput,
    ) -> anyhow::Result<ActorMigrationOutput> {
        // The DataCap actor -- needs to be created, and loading the verified
        // registry state

        // let verif_ref_state_v8 = store.get(verifreg)
        // let verified_clients = Hamt::load_with_bit_width(cid, &store,
        // DEFAULT_HAMT_BITWIDTH);

        todo!()
    }
}

/// Creates the Ethereum Account Manager actor in the state tree.
pub fn create_datacap_actor<BS: Blockstore + Clone + Send + Sync>(
    store: &BS,
    actors_out: &mut StateTree<BS>,
) -> anyhow::Result<()> {
    Ok(())
}
