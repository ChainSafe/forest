// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::{actors::account::State, address::Address};
use serde::{Deserialize, Serialize};

#[derive(Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct AccountStateLotusJson {
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    address: Address,
}

impl HasLotusJson for State {
    type LotusJson = AccountStateLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!({
                "Address": "f00"
            }),
            // Create a test account state
            Self::default_latest_version(Address::default().into()),
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        AccountStateLotusJson {
            address: self.pubkey_address(),
        }
    }

    // Always return the latest version when deserializing
    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        Self::default_latest_version(lotus_json.address.into())
    }
}
crate::test_snapshots!(State);
