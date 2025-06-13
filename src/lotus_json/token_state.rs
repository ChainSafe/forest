// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use super::*;
use crate::shim::econ::TokenAmount;
use ::cid::Cid;
use fil_actors_shared::frc46_token::token;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
#[schemars(rename = "TokenState")]
pub struct TokenStateLotusJson {
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub supply: TokenAmount,

    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    pub balances: Cid,

    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    pub allowances: Cid,

    pub hamt_bit_width: u32,
}

impl HasLotusJson for token::state::TokenState {
    type LotusJson = TokenStateLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!({
                "supply": "0",
                "balances": {"/":"baeaaaaa"},
                "allowances": {"/":"baeaaaaa"},
                "hamt_bit_width": 0
            }),
            token::state::TokenState {
                supply: TokenAmount::default().into(),
                balances: Cid::default(),
                allowances: Cid::default(),
                hamt_bit_width: 0,
            },
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        TokenStateLotusJson {
            supply: self.supply.into(),
            balances: self.balances,
            allowances: self.allowances,
            hamt_bit_width: self.hamt_bit_width,
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        token::state::TokenState {
            supply: lotus_json.supply.into(),
            balances: lotus_json.balances,
            allowances: lotus_json.allowances,
            hamt_bit_width: lotus_json.hamt_bit_width,
        }
    }
}
