// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{ActorMigration, ActorMigrationInput};
use crate::nv10::util::migrate_hamt_raw;
use crate::MigrationError;
use crate::MigrationOutput;
use crate::MigrationResult;
use actor_interface::actorv2::multisig::State as V2MultiSigState;
use actor_interface::actorv3::multisig::State as V3MultiSigState;
use actor::multisig::Transaction;
use async_std::sync::Arc;
use cid::Code;
use ipld_blockstore::BlockStore;

use actor_interface::actorv3;
use fil_types::ActorID;
use fil_types::HAMT_BIT_WIDTH;

pub struct MultisigMigrator;

// each actor's state migration is read from blockstore, changes state tree, and writes back to the blocstore.
impl<BS: BlockStore + Send + Sync> ActorMigration<BS> for MultisigMigrator {
    fn migrate_state(
        &self,
        store: Arc<BS>,
        input: ActorMigrationInput,
    ) -> MigrationResult<MigrationOutput> {
        let v2_in_state: Option<V2MultiSigState> = store
            .get(&input.head)
            .map_err(|e| MigrationError::BlockStoreRead(e.to_string()))?;

        let v2_in_state = v2_in_state.unwrap();

        // HAMT<addr.Address, Transaction>
        let pending_txns_out =
            migrate_hamt_raw::<_, Transaction>(&*store, v2_in_state.pending_txs, HAMT_BIT_WIDTH)?;

        let out_state = V2MultiSigState { // FIXME This should be v3. Getting an error about incompatible `next_tx_id` on using V3State.
            signers: v2_in_state.signers,
            num_approvals_threshold: v2_in_state.num_approvals_threshold,
            next_tx_id: v2_in_state.next_tx_id,
            
            initial_balance: v2_in_state.initial_balance,
            start_epoch: v2_in_state.start_epoch,
            unlock_duration: v2_in_state.unlock_duration,
        
            pending_txs: pending_txns_out,
        };

        let new_head = store.put(&out_state, Code::Blake2b256);

        Ok(MigrationOutput {
            new_code_cid: *actorv3::MULTISIG_ACTOR_CODE_ID,
            new_head: new_head.unwrap(),
        })
    }
}
