// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::actors::multisig::State;
use crate::shim::address::Address;
use crate::shim::clock::ChainEpoch;
use crate::shim::econ::TokenAmount;
use ::cid::Cid;

#[derive(Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
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

impl HasLotusJson for State {
    type LotusJson = MultisigStateLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!({
                "Signers": null,
                "NumApprovalsThreshold": 0,
                "NextTxnID": 0,
                "InitialBalance": "0",
                "StartEpoch": 0,
                "UnlockDuration": 0,
                "PendingTxns": {"/":"baeaaaaa"}
            }),
            State::default_latest_version(
                vec![],
                0,
                0,
                TokenAmount::default().into(),
                0,
                0,
                Cid::default(),
            ),
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        macro_rules! convert_multisig_state {
            ($($version:ident),+) => {
                match self {
                    $(
                        State::$version(state) => MultisigStateLotusJson {
                            signers: state.signers.iter().map(|addr| (*addr).into()).collect(),
                            num_approvals_threshold: state.num_approvals_threshold,
                            next_tx_id: state.next_tx_id.0,
                            initial_balance: state.initial_balance.clone().into(),
                            start_epoch: state.start_epoch,
                            unlock_duration: state.unlock_duration,
                            pending_txs: state.pending_txs,
                        },
                    )+
                }
            };
        }

        convert_multisig_state!(V8, V9, V10, V11, V12, V13, V14, V15, V16, V17)
    }

    // Always return the latest version when deserializing
    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        State::default_latest_version(
            lotus_json
                .signers
                .iter()
                .map(|addr| (*addr).into())
                .collect(),
            lotus_json.num_approvals_threshold,
            lotus_json.next_tx_id,
            lotus_json.initial_balance.into(),
            lotus_json.start_epoch,
            lotus_json.unlock_duration,
            lotus_json.pending_txs,
        )
    }
}
crate::test_snapshots!(State);
