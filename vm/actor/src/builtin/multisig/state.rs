// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::TxnID;
use address::Address;
use cid::Cid;
use clock::ChainEpoch;
use encoding::Cbor;
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
