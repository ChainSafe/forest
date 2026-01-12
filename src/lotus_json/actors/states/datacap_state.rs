// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::actors::datacap::State;
use crate::shim::address::Address;
use fil_actors_shared::frc46_token::token::state::TokenState;

#[derive(Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
#[schemars(rename = "DatacapState")]
pub struct DatacapStateLotusJson {
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub governor: Address,

    #[schemars(with = "LotusJson<TokenState>")]
    #[serde(with = "crate::lotus_json")]
    pub token: TokenState,
}

impl HasLotusJson for State {
    type LotusJson = DatacapStateLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!({
                "Governor": "f00",
                "Token": {
                    "Supply": "0",
                    "Balances": {"/":"baeaaaaa"},
                    "Allowances": {"/":"baeaaaaa"},
                    "HamtBitWidth": 0
                }
            }),
            State::default_latest_version(
                Address::default().into(),
                TokenState {
                    supply: Default::default(),
                    balances: Default::default(),
                    allowances: Default::default(),
                    hamt_bit_width: 0,
                },
            ),
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        macro_rules! convert_datacap_state {
            ($($version:ident),+) => {
                match self {
                    $(
                        State::$version(state) => {
                            DatacapStateLotusJson {
                                governor: state.governor.into(),
                                token: state.token,
                            }
                        },
                    )+
                }
            };
        }

        convert_datacap_state!(V9, V10, V11, V12, V13, V14, V15, V16, V17)
    }

    // Always return the latest version when deserializing
    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        State::default_latest_version(lotus_json.governor.into(), lotus_json.token)
    }
}
crate::test_snapshots!(State);
