// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! This module contains the logic for adding transient storage (EIP-1153).
//! actor. See the [FIP-0097](https://github.com/filecoin-project/FIPs/blob/b258e36e5e085afd48525cb6442f2301553df528/FIPS/fip-0097.md) for more details.

use crate::state_migration::common::{ActorMigration, ActorMigrationInput, ActorMigrationOutput};
use crate::utils::db::CborStoreExt as _;
use anyhow::Context as _;
use cid::Cid;
use fil_actor_evm_state::v15::State as EvmStateOld;
use fil_actor_evm_state::v16::State as EvmStateNew;
use fil_actor_evm_state::v16::Tombstone as TombstoneNew;
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

        let out_state = EvmStateNew {
            bytecode: in_state.bytecode,
            bytecode_hash: in_state
                .bytecode_hash
                .as_slice()
                .try_into()
                .context("bytecode hash conversion failed")?,
            contract_state: in_state.contract_state,
            nonce: in_state.nonce,
            tombstone: in_state.tombstone.map(|tombstone| TombstoneNew {
                origin: tombstone.origin,
                nonce: tombstone.nonce,
            }),
            transient_data: None,
        };

        let new_head = store.put_cbor_default(&out_state)?;

        Ok(Some(ActorMigrationOutput {
            new_code_cid: self.new_code_cid,
            new_head,
        }))
    }
}
