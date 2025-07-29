// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::lotus_json::{HasLotusJson, LotusJson};
use crate::shim::address::Address;
use fil_actor_datacap_state as datacap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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
