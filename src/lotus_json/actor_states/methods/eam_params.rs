// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use fvm_ipld_encoding::RawBytes;
use paste::paste;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct EAMCreateParamsLotusJson {
    #[schemars(with = "LotusJson<RawBytes>")]
    #[serde(with = "crate::lotus_json")]
    pub initcode: RawBytes,
    pub nonce: u64,
}

macro_rules! impl_eam_create_params {
    ($($version:literal),+) => {
        $(
            paste! {
                impl HasLotusJson for fil_actor_eam_state::[<v $version>]::CreateParams {
                    type LotusJson = EAMCreateParamsLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![
                            (
                                json!({
                                    "Initcode": "ESIzRFU=",
                                    "Nonce": 42
                                }),
                                Self {
                                    initcode: hex::decode("1122334455").unwrap(),
                                    nonce: 42,
                                },
                            ),
                        ]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        EAMCreateParamsLotusJson {
                            initcode: RawBytes::new(self.initcode),
                            nonce: self.nonce,
                        }
                    }

                    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                        Self {
                            initcode: lotus_json.initcode.into(),
                            nonce: lotus_json.nonce,
                        }
                    }
                }
            }
        )+
    };
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct EAMCreate2ParamsLotusJson {
    #[schemars(with = "LotusJson<Vec<u8>>")]
    #[serde(with = "crate::lotus_json")]
    pub initcode: Vec<u8>,
    pub salt: [u8; 32],
}

macro_rules! impl_eam_create2_params {
    ($($version:literal),+) => {
        $(
            paste! {
                impl HasLotusJson for fil_actor_eam_state::[<v $version>]::Create2Params {
                    type LotusJson = EAMCreate2ParamsLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![
                            (
                                json!({
                                    "Initcode": "ESIzRFU=",
                                    "Salt": [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]
                                }),
                                Self {
                                    initcode: hex::decode("1122334455").unwrap(),
                                    salt: [0; 32],
                                },
                            ),
                        ]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        EAMCreate2ParamsLotusJson {
                            initcode: self.initcode,
                            salt: self.salt,
                        }
                    }

                    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                        Self {
                            initcode: lotus_json.initcode,
                            salt: lotus_json.salt,
                        }
                    }
                }
            }
        )+
    };
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct EAMCreateExternalParamsLotusJson(
    #[schemars(with = "LotusJson<RawBytes>")]
    #[serde(with = "crate::lotus_json")]
    RawBytes,
);

macro_rules! impl_eam_create_external_params {
    ($($version:literal),+) => {
        $(
            paste! {
                impl HasLotusJson for fil_actor_eam_state::[<v $version>]::CreateExternalParams {
                    type LotusJson = EAMCreateExternalParamsLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![
                            (
                                json!("ESIzRFU="),
                                Self(hex::decode("1122334455").unwrap()),
                            ),
                        ]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        EAMCreateExternalParamsLotusJson(RawBytes::new(self.0))
                    }

                    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                        Self(lotus_json.0.into())
                    }
                }
            }
        )+
    };
}

impl_eam_create_params!(16, 15, 14, 13, 12, 11, 10);
impl_eam_create2_params!(16, 15, 14, 13, 12, 11, 10);
impl_eam_create_external_params!(16, 15, 14, 13, 12, 11, 10);
