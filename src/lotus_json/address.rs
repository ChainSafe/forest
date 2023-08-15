// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

use crate::shim::address::Address;

#[derive(Serialize, Deserialize, From, Into)]
pub struct AddressLotusJson(#[serde(with = "stringify")] Address);

impl HasLotusJson for Address {
    type LotusJson = AddressLotusJson;

    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(json!("f00"), Address::default())]
    }
}
