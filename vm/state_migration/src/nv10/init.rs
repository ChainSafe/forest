// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{ActorMigration, ActorMigrationInput};
use crate::nv10::util::migrate_hamt_raw;
use crate::MigrationError;
use crate::MigrationOutput;
use crate::MigrationResult;
use actor_interface::actorv2::init::State as V2InitState;
use actor_interface::actorv3::init::State as V3InitState;
use async_std::sync::Arc;
use cid::{Cid, Code};
use ipld_blockstore::BlockStore;
use fil_types::ActorID;
use fil_types::HAMT_BIT_WIDTH;

pub struct InitMigrator(Cid);

pub fn init_migrator_v3<BS: BlockStore + Send + Sync>(
    cid: Cid,
) -> Arc<dyn ActorMigration<BS> + Send + Sync> {
    Arc::new(InitMigrator(cid))
}

// each actor's state migration is read from blockstore, changes state tree, and writes back to the blocstore.
impl<BS: BlockStore + Send + Sync> ActorMigration<BS> for InitMigrator {
    fn migrate_state(
        &self,
        store: Arc<BS>,
        input: ActorMigrationInput,
    ) -> MigrationResult<MigrationOutput> {
        let v2_in_state: Option<V2InitState> = store
            .get(&input.head)
            .map_err(|e| MigrationError::BlockStoreRead(e.to_string()))?;

        let v2_in_state = v2_in_state.unwrap();

        // HAMT<addr.Address, abi.ActorID>
        let address_map_out =
            migrate_hamt_raw::<_, ActorID>(&*store, v2_in_state.address_map, HAMT_BIT_WIDTH); // FIXME: do we need cast as i32 here?

        let out_state = V3InitState {
            address_map: address_map_out.unwrap(),
            next_id: v2_in_state.next_id,
            network_name: v2_in_state.network_name,
        };

        let new_head = store.put(&out_state, Code::Blake2b256).map_err(|e| MigrationError::BlockStoreWrite(e.to_string()))?;

        Ok(MigrationOutput {
            new_code_cid: self.0,
            new_head
        })
    }
}
