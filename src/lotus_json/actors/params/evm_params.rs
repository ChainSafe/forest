// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use super::*;
use crate::rpc::eth::types::GetStorageAtParams;
use crate::shim::econ::TokenAmount;
use ::cid::Cid;
use fvm_ipld_encoding::RawBytes;
use pastey::paste;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct EVMConstructorParamsLotusJson {
    pub creator: [u8; 20],
    #[schemars(with = "LotusJson<RawBytes>")]
    #[serde(with = "crate::lotus_json")]
    pub initcode: RawBytes,
}

macro_rules! impl_evm_constructor_params {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_evm_constructor_params_ $version>] {
                    use super::*;
                    type T = fil_actor_evm_state::[<v $version>]::ConstructorParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = EVMConstructorParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![
                                (
                                    json!({
                                            "Creator": [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
                                            "Initcode": "ESIzRFU="
                                        }),
                                    Self {
                                        creator: fil_actor_evm_state::evm_shared::[<v $version>]::address::EthAddress([0; 20]),
                                        initcode: RawBytes::new(hex::decode("1122334455").unwrap()),
                                    },
                                ),
                            ]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            EVMConstructorParamsLotusJson {
                                creator: self.creator.0,
                                initcode: self.initcode,
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                creator: fil_actor_evm_state::evm_shared::[<v $version>]::address::EthAddress(lotus_json.creator),
                                initcode: lotus_json.initcode,
                            }
                        }
                    }
                }
            }
        )+
    };
}

impl_evm_constructor_params!(10, 11, 12, 13, 14, 15, 16, 17);

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct EVMDelegateCallParamsLotusJson {
    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    pub code: Cid,
    #[schemars(with = "LotusJson<RawBytes>")]
    #[serde(with = "crate::lotus_json")]
    pub input: RawBytes,
    pub caller: [u8; 20],
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub value: TokenAmount,
}

macro_rules! impl_evm_delegate_call_params_lotus_json {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_evm_delegate_call_params_ $version>] {
                    use super::*;
                    type T = fil_actor_evm_state::[<v $version>]::DelegateCallParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = EVMDelegateCallParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(
                                json!({
                                    "Code": {
                                        "/": "baeaaaaa"
                                    },
                                    "Input": "ESIzRFU=",
                                    "Caller": [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
                                    "Value": "0"
                                }),
                                Self {
                                    code: Cid::default(),
                                    input: hex::decode("1122334455").unwrap(),
                                    caller: fil_actor_evm_state::evm_shared::[<v $version>]::address::EthAddress([0; 20]),
                                    value: TokenAmount::from_atto(0).into(),
                                },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            EVMDelegateCallParamsLotusJson {
                                code: self.code,
                                input: self.input.into(),
                                caller: self.caller.0,
                                value: self.value.into(),
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                code: lotus_json.code,
                                input: lotus_json.input.into(),
                                caller: fil_actor_evm_state::evm_shared::[<v $version>]::address::EthAddress(lotus_json.caller),
                                value: lotus_json.value.into(),
                            }
                        }
                    }
                }
            }
        )+
    };
}

impl_evm_delegate_call_params_lotus_json!(10, 11, 12, 13, 14, 15, 16, 17);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct GetStorageAtParamsLotusJson {
    pub storage_key: [u8; 32],
}

impl HasLotusJson for GetStorageAtParams {
    type LotusJson = GetStorageAtParamsLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!({
                "StorageKey": [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 10]
            }),
            GetStorageAtParams::new(vec![0xa]).unwrap(),
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        GetStorageAtParamsLotusJson {
            storage_key: self.0,
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        GetStorageAtParams::new(lotus_json.storage_key.to_vec())
            .expect("expected array to have 32 elements")
    }
}
crate::test_snapshots!(GetStorageAtParams);
