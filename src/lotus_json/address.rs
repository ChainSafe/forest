// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

use crate::shim::address::Address;

impl HasLotusJson for Address {
    type LotusJson = Stringify<Address>;

    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(json!("f00"), Address::default())]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        self.into()
    }

    fn from_lotus_json(Stringify(address): Self::LotusJson) -> Self {
        address
    }
}
