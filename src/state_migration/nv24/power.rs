// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! This module contains the migration logic for the `NV19` upgrade for the
//! Power actor.

use crate::state_migration::common::{ActorMigration, ActorMigrationInput, ActorMigrationOutput};
use crate::utils::db::CborStoreExt as _;
use cid::Cid;
use fil_actor_power_state::v14::State as StateV14;
use fvm_ipld_blockstore::Blockstore;
use std::sync::Arc;

pub struct PowerMigrator(Cid);

pub(in crate::state_migration) fn power_migrator<BS: Blockstore>(
    cid: Cid,
) -> Arc<dyn ActorMigration<BS> + Send + Sync> {
    Arc::new(PowerMigrator(cid))
}

// original golang code: https://github.com/filecoin-project/go-state-types/blob/master/builtin/v11/migration/power.go
impl<BS: Blockstore> ActorMigration<BS> for PowerMigrator {
    fn migrate_state(
        &self,
        store: &BS,
        input: ActorMigrationInput,
    ) -> anyhow::Result<Option<ActorMigrationOutput>> {
        let in_state: StateV14 = store.get_cbor_required(&input.head)?;

        let out_state = StateV14 { ..in_state };

        let new_head = store.put_cbor_default(&out_state)?;

        Ok(Some(ActorMigrationOutput {
            new_code_cid: self.0,
            new_head,
        }))
    }
}
