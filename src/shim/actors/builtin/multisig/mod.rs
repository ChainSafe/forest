// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod ext;

use crate::shim::actors::convert::{
    from_address_v3_to_v2, from_address_v4_to_v2, from_token_v3_to_v2, from_token_v4_to_v2,
};
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::RawBytes;
use fvm_shared2::{address::Address, clock::ChainEpoch, econ::TokenAmount, MethodNum};
use serde::{Deserialize, Serialize};

/// Multisig actor method.
pub type Method = fil_actor_multisig_state::v8::Method;

/// Multisig actor state.
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
    /// Returns amount locked in multisig contract
    pub fn locked_balance(&self, height: ChainEpoch) -> anyhow::Result<TokenAmount> {
        Ok(match self {
            State::V8(st) => st.amount_locked(height - st.start_epoch),
            State::V9(st) => st.amount_locked(height - st.start_epoch),
            State::V10(st) => from_token_v3_to_v2(&st.amount_locked(height - st.start_epoch)),
            State::V11(st) => from_token_v3_to_v2(&st.amount_locked(height - st.start_epoch)),
            State::V12(st) => from_token_v4_to_v2(&st.amount_locked(height - st.start_epoch)),
            State::V13(st) => from_token_v4_to_v2(&st.amount_locked(height - st.start_epoch)),
            State::V14(st) => from_token_v4_to_v2(&st.amount_locked(height - st.start_epoch)),
            State::V15(st) => from_token_v4_to_v2(&st.amount_locked(height - st.start_epoch)),
            State::V16(st) => from_token_v4_to_v2(&st.amount_locked(height - st.start_epoch)),
        })
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
                crate::parse_pending_transactions!(res, txns);
                Ok(res)
            }
            State::V9(st) => {
                let txns = fil_actors_shared::v9::make_map_with_root::<
                    BS,
                    fil_actor_multisig_state::v9::Transaction,
                >(&st.pending_txs, store)?;
                crate::parse_pending_transactions!(res, txns);
                Ok(res)
            }
            State::V10(st) => {
                let txns = fil_actors_shared::v10::make_map_with_root::<
                    BS,
                    fil_actor_multisig_state::v10::Transaction,
                >(&st.pending_txs, store)?;
                crate::parse_pending_transactions_v3!(res, txns);
                Ok(res)
            }
            State::V11(st) => {
                let txns = fil_actors_shared::v11::make_map_with_root::<
                    BS,
                    fil_actor_multisig_state::v11::Transaction,
                >(&st.pending_txs, store)?;
                crate::parse_pending_transactions_v3!(res, txns);
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
                crate::parse_pending_transactions_v4!(res, txns);
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
                crate::parse_pending_transactions_v4!(res, txns);
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
                crate::parse_pending_transactions_v4!(res, txns);
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
                crate::parse_pending_transactions_v4!(res, txns);
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
                crate::parse_pending_transactions_v4!(res, txns);
                Ok(res)
            }
        }
    }
}
