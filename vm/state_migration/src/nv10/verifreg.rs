// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::ActorMigration;
use ipld_blockstore::BlockStore;
use async_std::sync::Arc;
use actor_interface::actorv2::verifreg::State as V2VerifRegState;
use actor_interface::actorv3::verifreg::State as V3VerifRegState;
use actor::verifreg::DataCap;
use crate::nv10::util::migrate_hamt_raw;
use fil_types::{SealVerifyInfo, HAMT_BIT_WIDTH};
use actor_interface::actorv3;
use crate::MigrationError;
use cid::Code;
use num_bigint::bigint_ser::BigIntDe;

use crate::{ActorMigrationInput, MigrationResult, MigrationOutput};

struct VerifregMigrator;

// each actor's state migration is read from blockstore, changes state tree, and writes back to the blocstore.
impl<BS: BlockStore + Send + Sync> ActorMigration<BS> for VerifregMigrator {
    fn migrate_state(
        &self,
        store: Arc<BS>,
        input: ActorMigrationInput,
    ) -> MigrationResult<MigrationOutput> {

        let v2_in_state: Option<V2VerifRegState> = store
            .get(&input.head)
            .map_err(|e| MigrationError::BlockStoreRead(e.to_string()))?;

        let v2_in_state = v2_in_state.unwrap();

        // HAMT[addr.Address]DataCap
        let verifiers_cid_out = migrate_hamt_raw::<_, BigIntDe>(&*store, v2_in_state.verifiers, HAMT_BIT_WIDTH)?;

        let verified_clients_cid_out = migrate_hamt_raw::<_, BigIntDe>(&*store, v2_in_state.verified_clients, HAMT_BIT_WIDTH)?;

        let out_state = V3VerifRegState {
            root_key: v2_in_state.root_key,
            verifiers: verifiers_cid_out, 
            verified_clients: verified_clients_cid_out 
        };

        let new_head = store.put(&out_state, Code::Blake2b256).map_err(|e| MigrationError::Other)?; // FIXME error handling
        
        Ok(MigrationOutput {
            new_code_cid: *actorv3::VERIFREG_ACTOR_CODE_ID,
            new_head,
        })
    }
}
