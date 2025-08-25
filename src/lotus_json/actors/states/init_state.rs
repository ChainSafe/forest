// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use super::*;
use crate::shim::actors::init::State;
use ::cid::Cid;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
#[schemars(rename = "InitState")]
pub struct InitStateLotusJson {
    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    pub address_map: Cid,
    #[serde(rename = "NextID")]
    pub next_id: u64,
    pub network_name: String,
}

macro_rules! impl_init_state_lotus_json {
    ($($version:ident),*) => {
        impl HasLotusJson for State {
            type LotusJson = InitStateLotusJson;

            #[cfg(test)]
            fn snapshots() -> Vec<(serde_json::Value, Self)> {
                vec![(
                    json!({
                        "AddressMap": {"/":"baeaaaaa"},
                        "NextID": 1,
                        "NetworkName": "testnet"
                    }),
                    State::V16(fil_actor_init_state::v16::State {
                        address_map: Cid::default(),
                        next_id: 1,
                        network_name: "testnet".to_string(),
                    }),
                )]
            }

            fn into_lotus_json(self) -> Self::LotusJson {
                match self {
                    $(
                      State::$version(state) => InitStateLotusJson {
                        address_map: state.address_map,
                        next_id: state.next_id,
                        network_name: state.network_name,
                    },
                    )*
                }
            }

            // Default to V16
            fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                State::V16(fil_actor_init_state::v16::State {
                    address_map: lotus_json.address_map,
                    next_id: lotus_json.next_id,
                    network_name: lotus_json.network_name,
                })
            }
        }
    };
}

// implement HasLotusJson for system::State for all versions
impl_init_state_lotus_json!(V16, V15, V14, V13, V12, V11, V10, V9, V8, V0);
