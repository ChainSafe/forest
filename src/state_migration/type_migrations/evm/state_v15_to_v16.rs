// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::state_migration::common::{TypeMigration, TypeMigrator};
use anyhow::Context as _;
use fil_actor_evm_state::{
    v15::State as EvmStateV15,
    v16::{State as EvmStateV16, Tombstone as TombstoneV16},
};
use fvm_ipld_blockstore::Blockstore;

impl TypeMigration<EvmStateV15, EvmStateV16> for TypeMigrator {
    fn migrate_type(in_state: EvmStateV15, _: &impl Blockstore) -> anyhow::Result<EvmStateV16> {
        let out_state = EvmStateV16 {
            bytecode: in_state.bytecode,
            bytecode_hash: in_state
                .bytecode_hash
                .as_slice()
                .try_into()
                .context("bytecode hash conversion failed")?,
            contract_state: in_state.contract_state,
            nonce: in_state.nonce,
            tombstone: in_state.tombstone.map(|t| TombstoneV16 {
                origin: t.origin,
                nonce: t.nonce,
            }),
            transient_data: None,
        };
        Ok(out_state)
    }
}
