// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::address::Address;
use jsonrpsee::core::Serialize;
use paste::paste;
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(transparent)]
pub struct AccountConstructorParamsLotusJson {
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub address: Address,
}

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

macro_rules!  impl_account_constructor_params {
    ($type_suffix:path: $($version:literal),+) => {
        $(
        paste! {
                 impl HasLotusJson for fil_actor_account_state::[<v $version>]::$type_suffix {
                    type LotusJson = AccountConstructorParamsLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![
                            (
                                json!("f01234"),
                                Self {
                                    address: Address::new_id(1234).into(),
                                },
                            ),
                        ]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        AccountConstructorParamsLotusJson { address: self.address.into() }
                    }

                    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                        Self { address: lotus_json.address.into() }
                    }
                }
            }
        )+
    };
}

// not added other versions because `fil_actor_account_state<version>::types`
// is private for all of them
impl_account_constructor_params!(types::ConstructorParams: 15, 16);
impl_account_constructor_params!(ConstructorParams: 11, 12, 13, 14);

// not added other versions because AuthenticateMessageParams is private for the rest of them
impl_account_authenticate_params!(types::AuthenticateMessageParams: 15, 16);
impl_account_authenticate_params!(AuthenticateMessageParams: 11, 12, 13, 14);
