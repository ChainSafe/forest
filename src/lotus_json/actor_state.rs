// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::{address::Address, econ::TokenAmount, state_tree::ActorState};
use ::cid::Cid;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ActorStateLotusJson {
    code: LotusJson<Cid>,
    head: LotusJson<Cid>,
    nonce: LotusJson<u64>,
    balance: LotusJson<TokenAmount>,
    #[serde(skip_serializing_if = "LotusJson::is_none", default)]
    delegated_address: LotusJson<Option<Address>>,
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

    fn into_lotus_json(self) -> Self::LotusJson {
        let fvm3::state_tree::ActorState {
            code,
            state,
            sequence,
            balance,
            delegated_address,
        } = From::from(self);
        Self::LotusJson {
            code: code.into(),
            head: state.into(),
            nonce: sequence.into(),
            balance: crate::shim::econ::TokenAmount::from(balance).into(),
            delegated_address: delegated_address
                .map(crate::shim::address::Address::from)
                .into(),
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        let ActorStateLotusJson {
            code,
            head,
            nonce,
            balance,
            delegated_address,
        } = lotus_json;
        Self::new(
            code.into_inner(),
            head.into_inner(),
            balance.into_inner(),
            nonce.into_inner(),
            delegated_address.into_inner(),
        )
    }
}
