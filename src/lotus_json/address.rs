// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::address::Address;

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "Address")]
pub struct AddressLotusJson(
    #[schemars(with = "String")]
    #[serde(with = "crate::lotus_json::stringify")]
    Address,
);

impl HasLotusJson for Address {
    type LotusJson = AddressLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(json!("f00"), Address::default())]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        AddressLotusJson(self)
    }

    fn from_lotus_json(AddressLotusJson(address): Self::LotusJson) -> Self {
        address
    }
}
