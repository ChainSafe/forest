// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use super::*;
use crate::shim::{actors::account, address::Address};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct AccountStateLotusJson {
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    address: Address,
}

impl HasLotusJson for account::State {
    type LotusJson = AccountStateLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!({
                "Address": "f00"
            }),
            // Create a test account state
            account::State::V16(fil_actor_account_state::v16::State {
                address: Address::default().into(),
            }),
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        AccountStateLotusJson {
            address: self.pubkey_address().into(),
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        // using V16 as a default version, because there is no way of knowing
        // which version data belongs to.
        account::State::V16(fil_actor_account_state::v16::State {
            address: lotus_json.address.into(),
        })
    }
}
