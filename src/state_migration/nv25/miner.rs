// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::state_migration::common::{
    ActorMigration, ActorMigrationInput, ActorMigrationOutput, TypeMigration, TypeMigrator,
};
use crate::utils::db::CborStoreExt as _;
use cid::Cid;
use fil_actor_miner_state::v15::State as MinerStateOld;
use fil_actor_miner_state::v16::State as MinerStateNew;
use fvm_ipld_blockstore::Blockstore;

pub struct MinerMigrator {
    pub new_code_cid: Cid,
}

impl<BS: Blockstore> ActorMigration<BS> for MinerMigrator {
    fn migrate_state(
        &self,
        store: &BS,
        input: ActorMigrationInput,
    ) -> anyhow::Result<Option<ActorMigrationOutput>> {
        let in_state: MinerStateOld = store.get_cbor_required(&input.head)?;
        let out_state: MinerStateNew = TypeMigrator::migrate_type(in_state, store)?;
        let new_head = store.put_cbor_default(&out_state)?;
        Ok(Some(ActorMigrationOutput {
            new_code_cid: self.new_code_cid,
            new_head,
        }))
    }
}
