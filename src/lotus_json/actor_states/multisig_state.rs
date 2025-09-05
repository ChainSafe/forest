// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::actors::multisig::State;
use crate::shim::address::Address;
use crate::shim::clock::ChainEpoch;
use crate::shim::econ::TokenAmount;
use ::cid::Cid;
use fil_actor_multisig_state::v16::TxnID;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
#[schemars(rename = "MultisigState")]
pub struct MultisigStateLotusJson {
    #[schemars(with = "LotusJson<Vec<Address>>")]
    #[serde(with = "crate::lotus_json")]
    pub signers: Vec<Address>,

    pub num_approvals_threshold: u64,

    #[serde(rename = "NextTxnID")]
    pub next_tx_id: i64,

    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub initial_balance: TokenAmount,

    #[schemars(with = "LotusJson<ChainEpoch>")]
    #[serde(with = "crate::lotus_json")]
    pub start_epoch: ChainEpoch,

    #[schemars(with = "LotusJson<ChainEpoch>")]
    #[serde(with = "crate::lotus_json")]
    pub unlock_duration: ChainEpoch,

    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json", rename = "PendingTxns")]
    pub pending_txs: Cid,
}

macro_rules! impl_multisig_state_lotus_json {
    ($($version:ident),*) => {
        impl HasLotusJson for State {
            type LotusJson = MultisigStateLotusJson;

            #[cfg(test)]
            fn snapshots() -> Vec<(serde_json::Value, Self)> {
                vec![(
                    json!({
                        "signers": [],
                        "num_approvals_threshold": 0,
                        "next_tx_id": 0,
                        "initial_balance": "0",
                        "start_epoch": 0,
                        "unlock_duration": 0,
                        "pending_txs": "0",
                    }),
                    State::V16(fil_actor_multisig_state::v16::State {
                        signers: vec![],
                        num_approvals_threshold: 0,
                        next_tx_id: TxnID(0),
                        initial_balance: TokenAmount::default().into(),
                        start_epoch: 0,
                        unlock_duration: 0,
                        pending_txs: Cid::default(),
                    })
                )]
            }

            fn into_lotus_json(self) -> Self::LotusJson {
                match self {
                    $(
                        State::$version(state) => MultisigStateLotusJson {
                            signers: state.signers.into_iter().map(|addr| addr.into()).collect(),
                            num_approvals_threshold: state.num_approvals_threshold,
                            next_tx_id: state.next_tx_id.0,
                            initial_balance: state.initial_balance.into(),
                            start_epoch: state.start_epoch,
                            unlock_duration: state.unlock_duration,
                            pending_txs: state.pending_txs,
                        },
                    )*
                }
            }

            // Default to V16
            fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                State::V16(fil_actor_multisig_state::v16::State{
                    signers: lotus_json.signers.into_iter().map(|addr| addr.into()).collect(),
                    num_approvals_threshold: lotus_json.num_approvals_threshold,
                    next_tx_id: TxnID(lotus_json.next_tx_id),
                    initial_balance: lotus_json.initial_balance.into(),
                    start_epoch: lotus_json.start_epoch,
                    unlock_duration: lotus_json.unlock_duration,
                    pending_txs: lotus_json.pending_txs,
                })
            }
        }
    };
}

impl_multisig_state_lotus_json!(V8, V9, V10, V11, V12, V13, V14, V15, V16, V17);
