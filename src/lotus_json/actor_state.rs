// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::{address::Address, econ::TokenAmount, state_tree::ActorState};
use ::cid::Cid;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ActorStateLotusJson {
    pub code: LotusJson<Cid>,
    pub head: LotusJson<Cid>,
    pub nonce: LotusJson<u64>,
    pub balance: LotusJson<TokenAmount>,
    pub delegated_address: Option<LotusJson<Address>>,
}

impl HasLotusJson for ActorState {
    type LotusJson = ActorStateLotusJson;

    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!({
                "Balance": "0",
                "Code": {
                    "/": "baeaaaaa"
                },
                "DelegatedAddress": null,
                "Head": {
                    "/": "baeaaaaa"
                },
                "Nonce": 0,
            }),
            Self::new(
                Cid::default(),
                Cid::default(),
                TokenAmount::default(),
                0,
                None,
            ),
        )]
    }
}

impl From<ActorStateLotusJson> for ActorState {
    fn from(value: ActorStateLotusJson) -> Self {
        let ActorStateLotusJson {
            code: LotusJson(code),
            head: LotusJson(head),
            nonce: LotusJson(nonce),
            balance: LotusJson(balance),
            delegated_address,
        } = value;
        Self::new(
            code,
            head,
            balance,
            nonce,
            delegated_address.map(LotusJson::into_inner),
        )
    }
}

impl From<ActorState> for ActorStateLotusJson {
    fn from(value: ActorState) -> Self {
        let fvm3::state_tree::ActorState {
            code,
            state,
            sequence,
            balance,
            delegated_address,
        } = From::from(value);
        Self {
            code: code.into(),
            head: state.into(),
            nonce: sequence.into(),
            balance: crate::shim::econ::TokenAmount::from(balance).into(),
            delegated_address: delegated_address
                .map(crate::shim::address::Address::from)
                .map(Into::into),
        }
    }
}
