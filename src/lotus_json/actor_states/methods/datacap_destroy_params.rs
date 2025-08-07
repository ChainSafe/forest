// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::lotus_json::{HasLotusJson, LotusJson};
use crate::shim::address::Address;
use crate::shim::econ::TokenAmount;
use fil_actor_datacap_state as datacap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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
