// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::nv10::util::migrate_hamt_raw;
use crate::MigrationError;
use crate::MigrationOutput;
use crate::MigrationResult;
use crate::{ActorMigration, ActorMigrationInput};
use actor::multisig::Transaction;
use actor_interface::actorv2::multisig::State as V2_MultiSigState;
use actor_interface::actorv3;
use actor_interface::actorv3::multisig::{State as V3_MultiSigState, TxnID as V3_TxnID};
use async_std::sync::Arc;
use cid::Code;
use fil_types::HAMT_BIT_WIDTH;
use ipld_blockstore::BlockStore;

pub struct MultisigMigrator;

impl<BS: BlockStore + Send + Sync> ActorMigration<BS> for MultisigMigrator {
    fn migrate_state(
        &self,
        store: Arc<BS>,
        input: ActorMigrationInput,
    ) -> MigrationResult<MigrationOutput> {
        let v2_in_state: V2_MultiSigState = store
            .get(&input.head)
            .map_err(|e| MigrationError::BlockStoreRead(e.to_string()))?
            .ok_or(MigrationError::StateNotFound)?;

        // HAMT<addr.Address, Transaction>
        let pending_txns_out =
            migrate_hamt_raw::<_, Transaction>(&*store, v2_in_state.pending_txs, HAMT_BIT_WIDTH)?;

        let out_state = V3_MultiSigState {
            signers: v2_in_state.signers,
            num_approvals_threshold: v2_in_state.num_approvals_threshold,
            next_tx_id: V3_TxnID(v2_in_state.next_tx_id.0),
            initial_balance: v2_in_state.initial_balance,
            start_epoch: v2_in_state.start_epoch,
            unlock_duration: v2_in_state.unlock_duration,
            pending_txs: pending_txns_out,
        };

        let new_head = store
            .put(&out_state, Code::Blake2b256)
            .map_err(|e| MigrationError::BlockStoreWrite(e.to_string()))?;

        Ok(MigrationOutput {
            new_code_cid: *actorv3::MULTISIG_ACTOR_CODE_ID,
            new_head,
        })
    }
}
