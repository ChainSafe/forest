// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{types::Transaction, TxnID};
use crate::make_map_with_root;
use address::Address;
use cid::Cid;
use clock::ChainEpoch;
use encoding::{tuple::*, Cbor};
use indexmap::IndexMap;
use ipld_blockstore::BlockStore;
use num_bigint::{bigint_ser, Integer};
use num_traits::Zero;
use std::error::Error as StdError;
use vm::TokenAmount;

/// Multisig actor state
#[derive(Serialize_tuple, Deserialize_tuple, Clone)]
pub struct State {
    pub signers: Vec<Address>,
    pub num_approvals_threshold: usize,
    pub next_tx_id: TxnID,

    // Linear unlock
    #[serde(with = "bigint_ser")]
    pub initial_balance: TokenAmount,
    pub start_epoch: ChainEpoch,
    pub unlock_duration: ChainEpoch,

    pub pending_txs: Cid,
}

impl State {
    /// Checks if `address` is in the list of signers
    pub fn is_signer(&self, address: &Address) -> bool {
        self.signers.contains(address)
    }

    /// Set locked amount in multisig state.
    pub fn set_locked(
        &mut self,
        start_epoch: ChainEpoch,
        unlock_duration: ChainEpoch,
        locked_amount: TokenAmount,
    ) {
        self.start_epoch = start_epoch;
        self.unlock_duration = unlock_duration;
        self.initial_balance = locked_amount;
    }

    /// Returns amount locked in multisig contract
    pub fn amount_locked(&self, elapsed_epoch: ChainEpoch) -> TokenAmount {
        if elapsed_epoch >= self.unlock_duration {
            return TokenAmount::from(0);
        }
        if elapsed_epoch <= 0 {
            return self.initial_balance.clone();
        }

        let remaining_lock_duration = self.unlock_duration - elapsed_epoch;

        // locked = ceil(InitialBalance * remainingLockDuration / UnlockDuration)
        let numerator: TokenAmount = &self.initial_balance * remaining_lock_duration;
        let denominator = TokenAmount::from(self.unlock_duration);

        numerator.div_ceil(&denominator)
    }

    /// Iterates all pending transactions and removes an address from each list of approvals,
    /// if present.  If an approval list becomes empty, the pending transaction is deleted.
    pub fn purge_approvals<BS: BlockStore>(
        &mut self,
        store: &BS,
        addr: &Address,
    ) -> Result<(), Box<dyn StdError>> {
        let mut txns = make_map_with_root(&self.pending_txs, store)?;

        // Identify transactions that need updating
        let mut txn_ids_to_purge = IndexMap::new();
        txns.for_each(|tx_id, txn: &Transaction| {
            for approver in txn.approved.iter() {
                if approver == addr {
                    txn_ids_to_purge.insert(tx_id.0.clone(), txn.clone());
                }
            }
            Ok(())
        })?;

        // Update or remove those transactions.
        for (tx_id, mut txn) in txn_ids_to_purge {
            txn.approved.retain(|approver| approver != addr);

            if !txn.approved.is_empty() {
                txns.set(tx_id.into(), txn)?;
            } else {
                txns.delete(&tx_id)?;
            }
        }

        self.pending_txs = txns.flush()?;

        Ok(())
    }

    pub(crate) fn check_available(
        &self,
        balance: TokenAmount,
        amount_to_spend: &TokenAmount,
        curr_epoch: ChainEpoch,
    ) -> Result<(), String> {
        if amount_to_spend < &0.into() {
            return Err(format!(
                "amount to spend {} less than zero",
                amount_to_spend
            ));
        }
        if &balance < amount_to_spend {
            return Err(format!(
                "current balance {} less than amount to spend {}",
                balance, amount_to_spend
            ));
        }

        if amount_to_spend.is_zero() {
            // Always permit a transaction that sends no value,
            // even if the lockup exceeds the current balance.
            return Ok(());
        }

        let remaining_balance = balance - amount_to_spend;
        let amount_locked = self.amount_locked(curr_epoch - self.start_epoch);
        if remaining_balance < amount_locked {
            return Err(format!(
                "actor balance {} if spent {} would be less than required locked amount {}",
                remaining_balance, amount_to_spend, amount_locked
            ));
        }
        Ok(())
    }
}

impl Cbor for State {}
