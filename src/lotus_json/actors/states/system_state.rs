// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::actors::system::State;
use ::cid::Cid;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
#[schemars(rename = "SystemState")]
pub struct SystemStateLotusJson {
    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    pub builtin_actors: Cid,
}

macro_rules! impl_system_state_lotus_json {
    ($($version:ident),*) => {
        impl HasLotusJson for State {
            type LotusJson = SystemStateLotusJson;

            #[cfg(test)]
            fn snapshots() -> Vec<(serde_json::Value, Self)> {
                vec![(
                     json!({
                         "builtin_actors": {
                            "/": "baeaaaaa"
                        },
                     }),
                    State::V16(fil_actor_system_state::v16::State {
                         builtin_actors: Default::default(),
                     })
                )]
            }

            fn into_lotus_json(self) -> Self::LotusJson {
                match self {
                    $(
                    State::$version(state) => SystemStateLotusJson {
                        builtin_actors: state.builtin_actors,
                    },
                    )*
                }
            }

            // Default to V16
            fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                State::V16(fil_actor_system_state::v16::State {
                    builtin_actors: lotus_json.builtin_actors,
                })
            }
        }
    };
}

// implement HasLotusJson for system::State for all versions
impl_system_state_lotus_json!(V16, V15, V14, V13, V12, V11, V10, V9, V8);
