// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::super::macros::{
    parse_pending_transactions, parse_pending_transactions_v3, parse_pending_transactions_v4,
};
use crate::{
    rpc::types::MsigVesting,
    shim::{MethodNum, address::Address, clock::ChainEpoch, econ::TokenAmount},
};
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::RawBytes;
use serde::{Deserialize, Serialize};
use spire_enum::prelude::delegated_enum;

/// Multisig actor method.
pub type Method = fil_actor_multisig_state::v8::Method;

/// Multisig actor state.
#[delegated_enum(impl_conversions)]
#[derive(Serialize, Debug)]
#[serde(untagged)]
pub enum State {
    V8(fil_actor_multisig_state::v8::State),
    V9(fil_actor_multisig_state::v9::State),
    V10(fil_actor_multisig_state::v10::State),
    V11(fil_actor_multisig_state::v11::State),
    V12(fil_actor_multisig_state::v12::State),
    V13(fil_actor_multisig_state::v13::State),
    V14(fil_actor_multisig_state::v14::State),
    V15(fil_actor_multisig_state::v15::State),
    V16(fil_actor_multisig_state::v16::State),
    V17(fil_actor_multisig_state::v17::State),
}

/// Transaction type used in multisig actor
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct Transaction {
    pub id: i64,
    pub to: Address,
    pub value: TokenAmount,
    pub method: MethodNum,
    pub params: RawBytes,
    pub approved: Vec<Address>,
}

impl State {
    pub fn default_latest_version(
        signers: Vec<fvm_shared4::address::Address>,
        num_approvals_threshold: u64,
        next_tx_id: i64,
        initial_balance: fvm_shared4::econ::TokenAmount,
        start_epoch: ChainEpoch,
        unlock_duration: ChainEpoch,
        pending_txs: cid::Cid,
    ) -> Self {
        State::V17(fil_actor_multisig_state::v17::State {
            signers,
            num_approvals_threshold,
            next_tx_id: fil_actor_multisig_state::v17::TxnID(next_tx_id),
            initial_balance,
            start_epoch,
            unlock_duration,
            pending_txs,
        })
    }

    /// Returns amount locked in multisig contract
    pub fn locked_balance(&self, height: ChainEpoch) -> anyhow::Result<TokenAmount> {
        Ok(delegate_state!(self => |st| st.amount_locked(height - st.start_epoch).into()))
    }

    /// Returns pending transactions for the given multisig wallet
    pub fn get_pending_txn<BS: Blockstore>(&self, store: &BS) -> anyhow::Result<Vec<Transaction>> {
        let mut res = Vec::new();
        match self {
            State::V8(st) => {
                let txns = fil_actors_shared::v8::make_map_with_root::<
                    BS,
                    fil_actor_multisig_state::v8::Transaction,
                >(&st.pending_txs, store)?;
                parse_pending_transactions!(res, txns);
                Ok(res)
            }
            State::V9(st) => {
                let txns = fil_actors_shared::v9::make_map_with_root::<
                    BS,
                    fil_actor_multisig_state::v9::Transaction,
                >(&st.pending_txs, store)?;
                parse_pending_transactions!(res, txns);
                Ok(res)
            }
            State::V10(st) => {
                let txns = fil_actors_shared::v10::make_map_with_root::<
                    BS,
                    fil_actor_multisig_state::v10::Transaction,
                >(&st.pending_txs, store)?;
                parse_pending_transactions_v3!(res, txns);
                Ok(res)
            }
            State::V11(st) => {
                let txns = fil_actors_shared::v11::make_map_with_root::<
                    BS,
                    fil_actor_multisig_state::v11::Transaction,
                >(&st.pending_txs, store)?;
                parse_pending_transactions_v3!(res, txns);
                Ok(res)
            }
            State::V12(st) => {
                let txns = fil_actor_multisig_state::v12::PendingTxnMap::load(
                    store,
                    &st.pending_txs,
                    fil_actor_multisig_state::v12::PENDING_TXN_CONFIG,
                    "pending txns",
                )
                .expect("Could not load pending transactions");
                parse_pending_transactions_v4!(res, txns);
                Ok(res)
            }
            State::V13(st) => {
                let txns = fil_actor_multisig_state::v13::PendingTxnMap::load(
                    store,
                    &st.pending_txs,
                    fil_actor_multisig_state::v13::PENDING_TXN_CONFIG,
                    "pending txns",
                )
                .expect("Could not load pending transactions");
                parse_pending_transactions_v4!(res, txns);
                Ok(res)
            }
            State::V14(st) => {
                let txns = fil_actor_multisig_state::v14::PendingTxnMap::load(
                    store,
                    &st.pending_txs,
                    fil_actor_multisig_state::v14::PENDING_TXN_CONFIG,
                    "pending txns",
                )
                .expect("Could not load pending transactions");
                parse_pending_transactions_v4!(res, txns);
                Ok(res)
            }
            State::V15(st) => {
                let txns = fil_actor_multisig_state::v15::PendingTxnMap::load(
                    store,
                    &st.pending_txs,
                    fil_actor_multisig_state::v15::PENDING_TXN_CONFIG,
                    "pending txns",
                )
                .expect("Could not load pending transactions");
                parse_pending_transactions_v4!(res, txns);
                Ok(res)
            }
            State::V16(st) => {
                let txns = fil_actor_multisig_state::v16::PendingTxnMap::load(
                    store,
                    &st.pending_txs,
                    fil_actor_multisig_state::v16::PENDING_TXN_CONFIG,
                    "pending txns",
                )
                .expect("Could not load pending transactions");
                parse_pending_transactions_v4!(res, txns);
                Ok(res)
            }
            State::V17(st) => {
                let txns = fil_actor_multisig_state::v17::PendingTxnMap::load(
                    store,
                    &st.pending_txs,
                    fil_actor_multisig_state::v17::PENDING_TXN_CONFIG,
                    "pending txns",
                )
                .expect("Could not load pending transactions");
                parse_pending_transactions_v4!(res, txns);
                Ok(res)
            }
        }
    }

    /// Returns the vesting schedule for this multisig state.
    pub fn get_vesting_schedule(&self) -> anyhow::Result<MsigVesting> {
        Ok(delegate_state!(self => |st| MsigVesting {
            initial_balance: st.initial_balance.atto().clone(),
            start_epoch: st.start_epoch,
            unlock_duration: st.unlock_duration,
        }))
    }
}
