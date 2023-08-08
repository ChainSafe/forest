// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::state_tree::ActorState;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ActorStateLotusJson {
    pub code: CidLotusJson,
    pub head: CidLotusJson,
    pub nonce: u64,
    pub balance: TokenAmountLotusJson,
    pub delegated_address: Option<AddressLotusJson>,
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
                ::cid::Cid::default(),
                ::cid::Cid::default(),
                crate::shim::econ::TokenAmount::default(),
                0,
                None,
            ),
        )]
    }
}

impl From<ActorStateLotusJson> for ActorState {
    fn from(value: ActorStateLotusJson) -> Self {
        let ActorStateLotusJson {
            code,
            head,
            nonce,
            balance,
            delegated_address,
        } = value;
        Self::new(
            code.into(),
            head.into(),
            balance.into(),
            nonce,
            delegated_address.map(Into::into),
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
            nonce: sequence,
            balance: crate::shim::econ::TokenAmount::from(balance).into(),
            delegated_address: delegated_address
                .map(crate::shim::address::Address::from)
                .map(Into::into),
        }
    }
}
