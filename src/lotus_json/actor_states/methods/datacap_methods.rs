// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::lotus_json::{HasLotusJson, LotusJson};
use crate::shim::address::Address;
use crate::shim::econ::TokenAmount;
use fil_actor_datacap_state as datacap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema)]
#[serde(transparent)]
pub struct DatacapBalanceParamsLotusJson {
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub address: Address,
}

macro_rules! impl_datacap_balance_params_lotus_json {
    ($($version:ident),*) => {
        $(
            impl HasLotusJson for datacap::$version::BalanceParams {
                type LotusJson = DatacapBalanceParamsLotusJson;

                #[cfg(test)]
                fn snapshots() -> Vec<(serde_json::Value, Self)> {
                    vec![(
                        serde_json::json!("f01234"),
                        datacap::$version::BalanceParams {
                            address: Address::new_id(1234).into(),
                        },
                    )]
                }

                fn into_lotus_json(self) -> Self::LotusJson {
                    DatacapBalanceParamsLotusJson { address: self.address.into() }
                }
                fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                    datacap::$version::BalanceParams { address: lotus_json.address.into() }
                }
            }
        )*
    };
}

impl_datacap_balance_params_lotus_json!(v11, v12, v13, v14, v15, v16);

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema)]
#[serde(transparent)]
pub struct DatacapConstructorParamsLotusJson {
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub governor: Address,
}

macro_rules! impl_datacap_constructor_params_lotus_json {
    ($($version:ident),*) => {
        $(
            impl HasLotusJson for datacap::$version::ConstructorParams {
                type LotusJson = DatacapConstructorParamsLotusJson;

                #[cfg(test)]
                fn snapshots() -> Vec<(serde_json::Value, Self)> {
                    vec![(
                        serde_json::json!("f01234"),
                        datacap::$version::ConstructorParams {
                            governor: Address::new_id(1234).into(),
                        },
                    )]
                }

                fn into_lotus_json(self) -> Self::LotusJson {
                    DatacapConstructorParamsLotusJson { governor: self.governor.into() }
                }
                fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                    datacap::$version::ConstructorParams { governor: lotus_json.governor.into() }
                }
            }
        )*
    };
}

impl_datacap_constructor_params_lotus_json!(v11, v12, v13, v14, v15, v16);

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct DatacapDestroyParamsLotusJson {
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub owner: Address,
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub amount: TokenAmount,
}

macro_rules! impl_datacap_destroy_params_lotus_json {
    ($($version:ident),*) => {
        $(
            impl HasLotusJson for datacap::$version::DestroyParams {
                type LotusJson = DatacapDestroyParamsLotusJson;

                #[cfg(test)]
                fn snapshots() -> Vec<(serde_json::Value, Self)> {
                    vec![(
                        serde_json::json!({
                            "owner": "f01234",
                            "amount": "1000000000000000000"
                        }),
                        datacap::$version::DestroyParams {
                            owner: Address::new_id(1234).into(),
                            amount: TokenAmount::from_atto(1_000_000_000_000_000_000_i64).into(),
                        },
                    )]
                }

                fn into_lotus_json(self) -> Self::LotusJson {
                    DatacapDestroyParamsLotusJson {
                        owner: self.owner.into(),
                        amount: self.amount.into(),
                    }
                }
                fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                    datacap::$version::DestroyParams {
                        owner: lotus_json.owner.into(),
                        amount: lotus_json.amount.into(),
                    }
                }
            }
        )*
    };
}

impl_datacap_destroy_params_lotus_json!(v9, v10, v11, v12, v13, v14, v15, v16);

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct DatacapMintParamsLotusJson {
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub to: Address,
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub amount: TokenAmount,
    #[schemars(with = "LotusJson<Vec<Address>>")]
    #[serde(with = "crate::lotus_json")]
    pub operators: Vec<Address>,
}

macro_rules! impl_datacap_mint_params_lotus_json {
    ($($version:ident),*) => {
        $(
            impl HasLotusJson for datacap::$version::MintParams {
                type LotusJson = DatacapMintParamsLotusJson;

                #[cfg(test)]
                fn snapshots() -> Vec<(serde_json::Value, Self)> {
                    vec![(
                        serde_json::json!({
                            "to": "f01234",
                            "amount": "1000000000000000000",
                            "operators": ["f01235", "f01236"]
                        }),
                        datacap::$version::MintParams {
                            to: Address::new_id(1234).into(),
                            amount: TokenAmount::from_atto(1_000_000_000_000_000_000_i64).into(),
                            operators: vec![
                                Address::new_id(1235).into(),
                                Address::new_id(1236).into(),
                            ],
                        },
                    )]
                }

                fn into_lotus_json(self) -> Self::LotusJson {
                    DatacapMintParamsLotusJson {
                        to: self.to.into(),
                        amount: self.amount.into(),
                        operators: self.operators.into_iter().map(|a| a.into()).collect(),
                    }
                }
                fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                    datacap::$version::MintParams {
                        to: lotus_json.to.into(),
                        amount: lotus_json.amount.into(),
                        operators: lotus_json.operators.into_iter().map(|a| a.into()).collect(),
                    }
                }
            }
        )*
    };
}

impl_datacap_mint_params_lotus_json!(v9, v10, v11, v12, v13, v14, v15, v16);
