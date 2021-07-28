// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::nv10::util::migrate_hamt_raw;
use crate::{
    ActorMigration, ActorMigrationInput, MigrationError, MigrationOutput, MigrationResult,
};
use actor_interface::actorv2::verifreg::State as V2_VerifRegState;
use actor_interface::actorv3::{verifreg::State as V3_VerifRegState};
use async_std::sync::Arc;
use cid::{Cid, Code};
use fil_types::HAMT_BIT_WIDTH;
use ipld_blockstore::BlockStore;
use num_bigint::bigint_ser::BigIntDe;

struct VerifregMigrator(Cid);

pub fn verifreg_migrator_v3<BS: BlockStore + Send + Sync>(
    cid: Cid,
) -> Arc<dyn ActorMigration<BS> + Send + Sync> {
    Arc::new(VerifregMigrator(cid))
}

impl<BS: BlockStore + Send + Sync> ActorMigration<BS> for VerifregMigrator {
    fn migrate_state(
        &self,
        store: Arc<BS>,
        input: ActorMigrationInput,
    ) -> MigrationResult<MigrationOutput> {
        let v2_in_state: V2_VerifRegState = store
            .get(&input.head)
            .map_err(|e| MigrationError::BlockStoreRead(e.to_string()))?
            .ok_or(MigrationError::StateNotFound)?;

        // HAMT[addr.Address]DataCap
        let verifiers_cid_out =
            migrate_hamt_raw::<_, BigIntDe>(&*store, v2_in_state.verifiers, HAMT_BIT_WIDTH)?;

        let verified_clients_cid_out =
            migrate_hamt_raw::<_, BigIntDe>(&*store, v2_in_state.verified_clients, HAMT_BIT_WIDTH)?;

        let out_state = V3_VerifRegState {
            root_key: v2_in_state.root_key,
            verifiers: verifiers_cid_out,
            verified_clients: verified_clients_cid_out,
        };

        let new_head = store
            .put(&out_state, Code::Blake2b256)
            .map_err(|e| MigrationError::BlockStoreWrite(e.to_string()))?;

        Ok(MigrationOutput {
            new_code_cid: self.0,
            new_head,
        })
    }
}
