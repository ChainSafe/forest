// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::{address::Address, econ::TokenAmount, state_tree::ActorState};
use ::cid::Cid;

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
#[schemars(rename = "ActorState")]
pub struct ActorStateLotusJson {
    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    code: Cid,
    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    head: Cid,
    nonce: u64,
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    balance: TokenAmount,
    #[schemars(with = "LotusJson<Option<Address>>")]
    #[serde(
        with = "crate::lotus_json",
        skip_serializing_if = "Option::is_none",
        default
    )]
    delegated_address: Option<Address>,
}

impl HasLotusJson for ActorState {
    type LotusJson = ActorStateLotusJson;

    #[cfg(test)]
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
            code,
            head: state,
            nonce: sequence,
            balance: crate::shim::econ::TokenAmount::from(balance),
            delegated_address: delegated_address.map(crate::shim::address::Address::from),
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
        Self::new(code, head, balance, nonce, delegated_address)
    }
}
