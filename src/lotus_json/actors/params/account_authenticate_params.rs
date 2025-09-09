// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use super::*;
use jsonrpsee::core::Serialize;
use paste::paste;
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct AuthenticateParamsLotusJson {
    #[schemars(with = "LotusJson<Vec<u8>>")]
    #[serde(with = "crate::lotus_json")]
    pub signature: Vec<u8>,
    #[schemars(with = "LotusJson<Vec<u8>>")]
    #[serde(with = "crate::lotus_json")]
    pub message: Vec<u8>,
}

macro_rules!  impl_account_authenticate_params {
    ($type_suffix:path: $($version:literal),+) => {
        $(
        paste! {
            impl HasLotusJson for fil_actor_account_state::[<v $version>]::$type_suffix {
                    type LotusJson = AuthenticateParamsLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![
                            (
                                json!({
                                    "Signature": null,
                                    "Message": null,
                                }),
                                Self {
                                   signature: vec![],
                                   message: vec![],
                                },
                            ),
                        ]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        AuthenticateParamsLotusJson {
                            signature: self.signature,
                            message: self.message,
                        }
                    }

                    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                        Self {
                            signature: lotus_json.signature,
                            message: lotus_json.message,
                        }
                    }
                }
            }
        )+
    };
}

// not added other versions because AuthenticateMessageParams is private for the rest of them
impl_account_authenticate_params!(types::AuthenticateMessageParams: 15, 16);
impl_account_authenticate_params!(AuthenticateMessageParams: 11, 12, 13, 14);
