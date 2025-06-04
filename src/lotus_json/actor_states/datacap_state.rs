// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::address::Address;
use fil_actors_shared::frc46_token::token::state::TokenState;
use crate::shim::actors::datacap::State;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
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

macro_rules! impl_data_cap_state_lotus_json {
    ($($version:ident), *) => {
        impl HasLotusJson for State {
            type LotusJson = DatacapStateLotusJson;
        
            #[cfg(test)]
            fn snapshots() -> Vec<(serde_json::Value, Self)> {
               vec![]
            }
        
            fn into_lotus_json(self) -> Self::LotusJson {
                match self {
                    $(
                        State::$version(state) => {
                            DatacapStateLotusJson {
                                governor: state.governor.into(),
                                token: state.token,
                            }
                        },
                    )*
                }
            }
        
            fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                State::V16(fil_actor_datacap_state::v16::State {
                    governor: lotus_json.governor.into(),
                    token: lotus_json.token,
                })
            }
        }     
    };
}

impl_data_cap_state_lotus_json!(V9, V10, V11, V12, V13, V14, V15, V16);
