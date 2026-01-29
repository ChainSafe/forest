// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! This module contains the logic for adding transient storage (EIP-1153).
//! actor. See the [FIP-0097](https://github.com/filecoin-project/FIPs/blob/b258e36e5e085afd48525cb6442f2301553df528/FIPS/fip-0097.md) for more details.

use crate::state_migration::common::{
    ActorMigration, ActorMigrationInput, ActorMigrationOutput, TypeMigration, TypeMigrator,
};
use crate::utils::db::CborStoreExt as _;
use cid::Cid;
use fil_actor_evm_state::v15::State as EvmStateOld;
use fil_actor_evm_state::v16::State as EvmStateNew;
use fvm_ipld_blockstore::Blockstore;

pub struct EvmMigrator {
    pub new_code_cid: Cid,
}

impl<BS: Blockstore> ActorMigration<BS> for EvmMigrator {
    fn migrate_state(
        &self,
        store: &BS,
        input: ActorMigrationInput,
    ) -> anyhow::Result<Option<ActorMigrationOutput>> {
        let in_state: EvmStateOld = store.get_cbor_required(&input.head)?;
        let out_state: EvmStateNew = TypeMigrator::migrate_type(in_state, store)?;
        let new_head = store.put_cbor_default(&out_state)?;
        Ok(Some(ActorMigrationOutput {
            new_code_cid: self.new_code_cid,
            new_head,
        }))
    }
}
