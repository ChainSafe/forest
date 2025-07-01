// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use jsonrpsee::core::Serialize;
use paste::paste;
use schemars::JsonSchema;
use serde::Deserialize;
use std::fmt::Debug;

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct InitConstructorParamsLotusJson {
    pub network_name: String,
}

macro_rules! impl_lotus_json_for_init_constructor_params {
    ($($version:literal),+) => {
        $(
            paste! {
                impl HasLotusJson for fil_actor_init_state::[<v $version>]::ConstructorParams {
                    type LotusJson = InitConstructorParamsLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![
                            (
                                json!({
                                    "NetworkName": "calibnet",
                                }),
                                Self {
                                    network_name: "calibnet".to_string(),
                                },
                            ),
                        ]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        Self::LotusJson {
                            network_name: self.network_name,
                        }
                    }

                    fn from_lotus_json(json: Self::LotusJson) -> Self {
                        Self {
                            network_name: json.network_name,
                        }
                    }
                }
            }
        )+
    }
}

impl_lotus_json_for_init_constructor_params!(8, 9, 10, 11, 12, 13, 14, 15, 16);
