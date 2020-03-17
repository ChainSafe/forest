// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{Transaction, TxnID};
use address::Address;
use cid::Cid;
use clock::ChainEpoch;
use encoding::Cbor;
use ipld_blockstore::BlockStore;
use ipld_hamt::Hamt;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use vm::TokenAmount;

/// Multisig actor state
pub struct State {
    pub signers: Vec<Address>,
    pub num_approvals_threshold: i64,
    pub next_tx_id: TxnID,

    // Linear unlock
    pub initial_balance: TokenAmount,
    pub start_epoch: ChainEpoch,
    pub unlock_duration: ChainEpoch,

    pub pending_txs: Cid,
}

impl State {
    /// Returns amount locked in multisig contract
    pub fn amount_locked(&self, elapsed_epoch: ChainEpoch) -> TokenAmount {
        if elapsed_epoch >= self.unlock_duration {
            return TokenAmount::new(0);
        }
        let unit_locked = self.initial_balance.0.clone() / self.unlock_duration.0;
        TokenAmount(unit_locked * (self.unlock_duration.0 - elapsed_epoch.0))
    }

    pub(crate) fn is_signer(&self, addr: &Address) -> bool {
        for s in &self.signers {
            if addr == s {
                return true;
            }
        }
        false
    }

    pub(crate) fn check_available(
        &self,
        balance: TokenAmount,
        amount_to_spend: TokenAmount,
        curr_epoch: ChainEpoch,
    ) -> Result<(), String> {
        // * Note `< 0` check skipped because `TokenAmount` is big uint
        if balance.0 < amount_to_spend.0 {
            return Err(format!(
                "current balance {} less than amount to spend {}",
                balance.0, amount_to_spend.0
            ));
        }

        let remaining_balance = balance.0 - amount_to_spend.0;
        let amount_locked = self.amount_locked(curr_epoch - self.start_epoch);
        if remaining_balance < amount_locked.0 {
            return Err(format!(
                "actor balance if spent {} would be less than required locked amount {}",
                remaining_balance, amount_locked.0
            ));
        }
        Ok(())
    }

    pub(crate) fn get_pending_transaction<BS: BlockStore>(
        &self,
        s: &BS,
        txn_id: TxnID,
    ) -> Result<Transaction, String> {
        let map: Hamt<String, _> = Hamt::load_with_bit_width(&self.pending_txs, s, 5)?;
        match map.get(&txn_id.key()) {
            Ok(Some(tx)) => Ok(tx),
            Ok(None) => Err(format!(
                "failed to find transaction {} in HAMT {}",
                txn_id.0, self.pending_txs
            )),
            Err(e) => Err(format!("failed to read transaction: {}", e)),
        }
    }

    pub(crate) fn put_pending_transaction<BS: BlockStore>(
        &mut self,
        s: &BS,
        txn_id: TxnID,
        txn: Transaction,
    ) -> Result<(), String> {
        let mut map: Hamt<String, _> = Hamt::load_with_bit_width(&self.pending_txs, s, 5)?;
        map.set(txn_id.key(), txn)?;
        self.pending_txs = map.flush()?;
        Ok(())
    }

    pub(crate) fn delete_pending_transaction<BS: BlockStore>(
        &mut self,
        s: &BS,
        txn_id: TxnID,
    ) -> Result<(), String> {
        let mut map: Hamt<String, _> = Hamt::load_with_bit_width(&self.pending_txs, s, 5)?;
        map.delete(&txn_id.key())?;
        self.pending_txs = map.flush()?;
        Ok(())
    }
}

impl Serialize for State {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (
            &self.signers,
            &self.num_approvals_threshold,
            &self.next_tx_id,
            &self.initial_balance,
            &self.start_epoch,
            &self.unlock_duration,
            &self.pending_txs,
        )
            .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for State {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (
            signers,
            num_approvals_threshold,
            next_tx_id,
            initial_balance,
            start_epoch,
            unlock_duration,
            pending_txs,
        ) = Deserialize::deserialize(deserializer)?;
        Ok(Self {
            signers,
            num_approvals_threshold,
            next_tx_id,
            initial_balance,
            start_epoch,
            unlock_duration,
            pending_txs,
        })
    }
}

impl Cbor for State {}
