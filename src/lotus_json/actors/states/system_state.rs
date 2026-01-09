// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::actors::system::State;
use ::cid::Cid;

#[derive(Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
#[schemars(rename = "SystemState")]
pub struct SystemStateLotusJson {
    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    pub builtin_actors: Cid,
}

impl HasLotusJson for State {
    type LotusJson = SystemStateLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!({
                "BuiltinActors": {
                   "/": "baeaaaaa"
               },
            }),
            State::default_latest_version(Default::default()),
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        macro_rules! convert_system_state {
            ($($version:ident),+) => {
                match self {
                    $(
                        State::$version(state) => SystemStateLotusJson {
                            builtin_actors: state.builtin_actors,
                        },
                    )+
                }
            };
        }

        convert_system_state!(V8, V9, V10, V11, V12, V13, V14, V15, V16, V17)
    }

    // Always return the latest version when deserializing
    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        State::default_latest_version(lotus_json.builtin_actors)
    }
}
crate::test_snapshots!(State);
