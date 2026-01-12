// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::actors::init::State;
use ::cid::Cid;

#[derive(Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
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
            State::default_latest_version(Cid::default(), 1, "testnet".to_string()),
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        macro_rules! convert_init_state {
            ($($version:ident),+) => {
                match self {
                    $(
                        State::$version(state) => InitStateLotusJson {
                            address_map: state.address_map,
                            next_id: state.next_id,
                            network_name: state.network_name,
                        },
                    )+
                }
            };
        }

        convert_init_state!(V0, V8, V9, V10, V11, V12, V13, V14, V15, V16, V17)
    }

    // Always return the latest version when deserializing
    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        State::default_latest_version(
            lotus_json.address_map,
            lotus_json.next_id,
            lotus_json.network_name,
        )
    }
}
crate::test_snapshots!(State);
