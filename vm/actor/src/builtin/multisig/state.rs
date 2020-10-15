// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::TxnID;
use address::Address;
use cid::Cid;
use clock::ChainEpoch;
use encoding::{tuple::*, Cbor};
use num_bigint::{bigint_ser, Integer};
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
        if elapsed_epoch < 0 {
            return self.initial_balance.clone();
        }
        // Division truncation is broken here: https://github.com/filecoin-project/specs-actors/issues/1131
        let unit_locked: TokenAmount = self
            .initial_balance
            .div_floor(&TokenAmount::from(self.unlock_duration));
        unit_locked * (self.unlock_duration - elapsed_epoch)
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
