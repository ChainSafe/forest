// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::{
    address::Address,
    econ::TokenAmount,
    state_tree::{ActorState, ActorState_latest},
};
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
        ),
        (
            json!({
                "Balance": "123456789012345678901234567890",
                "Code": {
                    "/": "baeaaaaa"
                },
                "Head": {
                    "/": "baeaaaaa"
                },
                "Nonce": 7,
                "DelegatedAddress": "f410fgaytemzugu3doobzmfrggzdfmztwq2lkevnyy5i",
            }),
            Self::new(
                Cid::default(),
                Cid::default(),
                TokenAmount::from_atto(123456789012345678901234567890u128),
                7,
                Some(Address::new_delegated(10, b"0123456789abcdefghij").unwrap()),
            ),
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        let ActorState_latest {
            code,
            state,
            sequence,
            balance,
            delegated_address,
        } = self.into();
        Self::LotusJson {
            code,
            head: state,
            nonce: sequence,
            balance: balance.into(),
            delegated_address: delegated_address.map(Into::into),
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
