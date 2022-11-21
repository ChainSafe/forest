use std::{collections::HashSet, iter::FromIterator};

use anyhow::anyhow;
use fil_actors_runtime_v8::{Map, MessageAccumulator};
use fvm_ipld_blockstore::Blockstore;
use fvm_shared::address::Address;
use integer_encoding::VarInt;

use crate::{State, Transaction, TxnID, SIGNERS_MAX};

pub struct StateSummary {
    pub pending_tx_count: u64,
    pub num_approvals_threshold: u64,
    pub signer_count: usize,
}

/// Checks internal invariants of multisig state.
pub fn check_state_invariants<BS: Blockstore>(
    state: &State,
    store: &BS,
) -> (StateSummary, MessageAccumulator) {
    let acc = MessageAccumulator::default();

    // assert invariants involving signers
    acc.require(
        state.signers.len() <= SIGNERS_MAX,
        format!("multisig has too many signers: {}", state.signers.len()),
    );
    acc.require(
        state.signers.len() as u64 >= state.num_approvals_threshold,
        format!(
            "multisig has insufficient signers to meet threshold ({} < {})",
            state.signers.len(),
            state.num_approvals_threshold
        ),
    );

    // See https://github.com/filecoin-project/specs-actors/issues/1185
    if state.unlock_duration == 0 {
        acc.require(
            state.start_epoch == 0,
            format!(
                "non-zero start epoch {} with zero unlock duration",
                state.start_epoch
            ),
        );
        acc.require(
            state.initial_balance.is_zero(),
            format!(
                "non-zero locked balance {} with zero unlock duration",
                state.initial_balance
            ),
        );
    }

    // create lookup to test transaction approvals are multisig signers
    let signers = HashSet::<&Address>::from_iter(state.signers.iter());

    // test pending transactions
    let mut max_tx_id = TxnID(-1);
    let mut pending_tx_count = 0u64;

    match Map::<_, Transaction>::load(&state.pending_txs, store) {
        Ok(transactions) => {
            let ret = transactions.for_each(|tx_id, transaction| {
                let tx_id = TxnID(
                    i64::decode_var(tx_id)
                        .ok_or_else(|| anyhow!("failed to decode key: {:?}", tx_id))?
                        .0,
                );

                if tx_id > max_tx_id {
                    max_tx_id = tx_id;
                }

                let mut seen_approvals = HashSet::<&Address>::new();
                transaction.approved.iter().for_each(|approval| {
                    acc.require(
                        signers.contains(approval),
                        format!(
                            "approval {approval} for transaction {tx_id} is not in signers list"
                        ),
                    );

                    acc.require(
                        !seen_approvals.contains(approval),
                        format!("duplicate approval {approval} for transaction {tx_id}"),
                    );
                    seen_approvals.insert(approval);
                });
                acc.require((seen_approvals.len() as u64) < state.num_approvals_threshold,
                    format!("number of approvals ({}) meets the approvals threshold ({}), transaction should not be pending",
                    seen_approvals.len(), state.num_approvals_threshold));

                pending_tx_count += 1;

                Ok(())
            });

            acc.require_no_error(ret, "error iterating transactions");
        }
        Err(e) => acc.add(format!("error loading transactions: {e}")),
    };

    acc.require(
        state.next_tx_id > max_tx_id,
        format!(
            "next transaction id {} is not greater than pending ids",
            state.next_tx_id
        ),
    );

    (
        StateSummary {
            pending_tx_count,
            num_approvals_threshold: state.num_approvals_threshold,
            signer_count: state.signers.len(),
        },
        acc,
    )
}
