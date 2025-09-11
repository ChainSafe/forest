// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use ::cid::Cid;
use fvm_ipld_encoding::RawBytes;
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

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct InitExecParamsLotusJson {
    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    #[serde(rename = "CodeCID")]
    pub code_cid: Cid,

    #[schemars(with = "LotusJson<RawBytes>")]
    #[serde(with = "crate::lotus_json")]
    pub constructor_params: RawBytes,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct InitExec4ParamsLotusJson {
    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    #[serde(rename = "CodeCID")]
    pub code_cid: Cid,

    #[schemars(with = "LotusJson<RawBytes>")]
    #[serde(with = "crate::lotus_json")]
    pub constructor_params: RawBytes,

    #[schemars(with = "LotusJson<RawBytes>")]
    #[serde(with = "crate::lotus_json")]
    pub sub_address: RawBytes,
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

macro_rules! impl_lotus_json_for_init_exec_params {
    ($($version:literal),+) => {
        $(
            paste! {
                impl HasLotusJson for fil_actor_init_state::[<v $version>]::ExecParams {
                    type LotusJson = InitExecParamsLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![
                            (
                                json!({
                                    "CodeCID": {
                                        "/": "baeaaaaa"
                                    },
                                    "ConstructorParams": "ESIzRFU=",
                                }),
                                Self {
                                    code_cid: Cid::default(),
                                    constructor_params: RawBytes::new(hex::decode("1122334455").unwrap()),
                                },
                            ),
                        ]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        Self::LotusJson {
                            code_cid: self.code_cid,
                            constructor_params: self.constructor_params,
                        }
                    }

                    fn from_lotus_json(json: Self::LotusJson) -> Self {
                        Self {
                            code_cid: json.code_cid,
                            constructor_params: json.constructor_params,
                        }
                    }
                }
            }
        )+
    };
}

macro_rules! impl_lotus_json_for_init_exec4_params {
    ($($version:literal),+) => {
        $(
            paste! {
                impl HasLotusJson for fil_actor_init_state::[<v $version>]::Exec4Params {
                    type LotusJson = InitExec4ParamsLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![
                            (
                                json!({
                                    "CodeCID": {
                                        "/": "baeaaaaa"
                                    },
                                    "ConstructorParams": "ESIzRFU=",
                                    "SubAddress": "ESIzRFU=",
                                }),
                                Self {
                                    code_cid: Cid::default(),
                                    constructor_params: RawBytes::new(hex::decode("1122334455").unwrap()),
                                    subaddress: RawBytes::new(hex::decode("1122334455").unwrap()),
                                },
                            ),
                        ]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        Self::LotusJson {
                            code_cid: self.code_cid,
                            constructor_params: self.constructor_params,
                            sub_address: self.subaddress,
                        }
                    }

                    fn from_lotus_json(json: Self::LotusJson) -> Self {
                        Self {
                            code_cid: json.code_cid,
                            constructor_params: json.constructor_params,
                            subaddress: json.sub_address,
                        }
                    }
                }
            }
        )+
    };
}

impl_lotus_json_for_init_constructor_params!(8, 9, 10, 11, 12, 13, 14, 15, 16, 17);
impl_lotus_json_for_init_exec_params!(8, 9, 10, 11, 12, 13, 14, 15, 16, 17);
impl_lotus_json_for_init_exec4_params!(10, 11, 12, 13, 14, 15, 16, 17);
