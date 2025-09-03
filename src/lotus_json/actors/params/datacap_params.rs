// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::lotus_json::{HasLotusJson, LotusJson};
use crate::shim::address::Address;
use crate::shim::econ::TokenAmount;
use fil_actor_datacap_state as datacap;
use fil_actors_shared::frc46_token::token::types::{
    BurnFromParams, BurnParams, DecreaseAllowanceParams, GetAllowanceParams,
    IncreaseAllowanceParams, RevokeAllowanceParams, TransferFromParams, TransferParams,
};
use fvm_ipld_encoding::RawBytes;
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

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct TransferParamsLotusJson {
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub to: Address,
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub amount: TokenAmount,
    #[schemars(with = "LotusJson<RawBytes>")]
    #[serde(with = "crate::lotus_json")]
    pub operator_data: RawBytes,
}

impl HasLotusJson for TransferParams {
    type LotusJson = TransferParamsLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            serde_json::json!({
                "to": "f01234",
                "amount": "1000000000000000000",
                "operator_data": "dGVzdCBkYXRh"
            }),
            TransferParams {
                to: Address::new_id(1234).into(),
                amount: TokenAmount::from_atto(1_000_000_000_000_000_000_i64).into(),
                operator_data: RawBytes::new(b"test data".to_vec()),
            },
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        TransferParamsLotusJson {
            to: self.to.into(),
            amount: self.amount.into(),
            operator_data: self.operator_data,
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        TransferParams {
            to: lotus_json.to.into(),
            amount: lotus_json.amount.into(),
            operator_data: lotus_json.operator_data,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct TransferFromParamsLotusJson {
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub from: Address,
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub to: Address,
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub amount: TokenAmount,
    #[schemars(with = "LotusJson<RawBytes>")]
    #[serde(with = "crate::lotus_json")]
    pub operator_data: RawBytes,
}

impl HasLotusJson for TransferFromParams {
    type LotusJson = TransferFromParamsLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            serde_json::json!({
                "from": "f01234",
                "to": "f01235",
                "amount": "1000000000000000000",
                "operator_data": "dGVzdCBkYXRh"
            }),
            TransferFromParams {
                from: Address::new_id(1234).into(),
                to: Address::new_id(1235).into(),
                amount: TokenAmount::from_atto(1_000_000_000_000_000_000_i64).into(),
                operator_data: RawBytes::new(b"test data".to_vec()),
            },
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        TransferFromParamsLotusJson {
            from: self.from.into(),
            to: self.to.into(),
            amount: self.amount.into(),
            operator_data: self.operator_data,
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        TransferFromParams {
            from: lotus_json.from.into(),
            to: lotus_json.to.into(),
            amount: lotus_json.amount.into(),
            operator_data: lotus_json.operator_data,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct IncreaseAllowanceParamsLotusJson {
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub operator: Address,
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub increase: TokenAmount,
}

impl HasLotusJson for IncreaseAllowanceParams {
    type LotusJson = IncreaseAllowanceParamsLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            serde_json::json!({
                "operator": "f01234",
                "increase": "1000000000000000000"
            }),
            IncreaseAllowanceParams {
                operator: Address::new_id(1234).into(),
                increase: TokenAmount::from_atto(1_000_000_000_000_000_000_i64).into(),
            },
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        IncreaseAllowanceParamsLotusJson {
            operator: self.operator.into(),
            increase: self.increase.into(),
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        IncreaseAllowanceParams {
            operator: lotus_json.operator.into(),
            increase: lotus_json.increase.into(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct DecreaseAllowanceParamsLotusJson {
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub operator: Address,
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub decrease: TokenAmount,
}

impl HasLotusJson for DecreaseAllowanceParams {
    type LotusJson = DecreaseAllowanceParamsLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            serde_json::json!({
                "operator": "f01234",
                "decrease": "1000000000000000000"
            }),
            DecreaseAllowanceParams {
                operator: Address::new_id(1234).into(),
                decrease: TokenAmount::from_atto(1_000_000_000_000_000_000_i64).into(),
            },
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        DecreaseAllowanceParamsLotusJson {
            operator: self.operator.into(),
            decrease: self.decrease.into(),
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        DecreaseAllowanceParams {
            operator: lotus_json.operator.into(),
            decrease: lotus_json.decrease.into(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct RevokeAllowanceParamsLotusJson {
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub operator: Address,
}

impl HasLotusJson for RevokeAllowanceParams {
    type LotusJson = RevokeAllowanceParamsLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            serde_json::json!({
                "operator": "f01234"
            }),
            RevokeAllowanceParams {
                operator: Address::new_id(1234).into(),
            },
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        RevokeAllowanceParamsLotusJson {
            operator: self.operator.into(),
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        RevokeAllowanceParams {
            operator: lotus_json.operator.into(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct BurnParamsLotusJson {
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub amount: TokenAmount,
}

impl HasLotusJson for BurnParams {
    type LotusJson = BurnParamsLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            serde_json::json!({
                "amount": "1000000000000000000"
            }),
            BurnParams {
                amount: TokenAmount::from_atto(1_000_000_000_000_000_000_i64).into(),
            },
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        BurnParamsLotusJson {
            amount: self.amount.into(),
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        BurnParams {
            amount: lotus_json.amount.into(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct BurnFromParamsLotusJson {
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub owner: Address,
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub amount: TokenAmount,
}

impl HasLotusJson for BurnFromParams {
    type LotusJson = BurnFromParamsLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            serde_json::json!({
                "owner": "f01234",
                "amount": "1000000000000000000"
            }),
            BurnFromParams {
                owner: Address::new_id(1234).into(),
                amount: TokenAmount::from_atto(1_000_000_000_000_000_000_i64).into(),
            },
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        BurnFromParamsLotusJson {
            owner: self.owner.into(),
            amount: self.amount.into(),
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        BurnFromParams {
            owner: lotus_json.owner.into(),
            amount: lotus_json.amount.into(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct GetAllowanceParamsLotusJson {
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub owner: Address,
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub operator: Address,
}

impl HasLotusJson for GetAllowanceParams {
    type LotusJson = GetAllowanceParamsLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            serde_json::json!({
                "owner": "f01234",
                "operator": "f01235"
            }),
            GetAllowanceParams {
                owner: Address::new_id(1234).into(),
                operator: Address::new_id(1235).into(),
            },
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        GetAllowanceParamsLotusJson {
            owner: self.owner.into(),
            operator: self.operator.into(),
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        GetAllowanceParams {
            owner: lotus_json.owner.into(),
            operator: lotus_json.operator.into(),
        }
    }
}
