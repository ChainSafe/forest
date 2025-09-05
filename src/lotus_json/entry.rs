// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use super::*;
use crate::shim::actors::cron::Entry;
use crate::shim::address::Address;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct EntryLotusJson {
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub receiver: Address,
    pub method_num: u64,
}

impl HasLotusJson for Entry {
    type LotusJson = EntryLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!({
                "Receiver": "f00",
                "MethodNum": 10
            }),
            // Create a test entry
            Entry::V16(fil_actor_cron_state::v16::Entry {
                receiver: Default::default(),
                method_num: 0,
            }),
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        match self {
            Entry::V8(e) => EntryLotusJson {
                receiver: e.receiver.into(),
                method_num: e.method_num,
            },
            Entry::V9(e) => EntryLotusJson {
                receiver: e.receiver.into(),
                method_num: e.method_num,
            },
            Entry::V10(e) => EntryLotusJson {
                receiver: e.receiver.into(),
                method_num: e.method_num,
            },
            Entry::V11(e) => EntryLotusJson {
                receiver: e.receiver.into(),
                method_num: e.method_num,
            },
            Entry::V12(e) => EntryLotusJson {
                receiver: e.receiver.into(),
                method_num: e.method_num,
            },
            Entry::V13(e) => EntryLotusJson {
                receiver: e.receiver.into(),
                method_num: e.method_num,
            },
            Entry::V14(e) => EntryLotusJson {
                receiver: e.receiver.into(),
                method_num: e.method_num,
            },
            Entry::V15(e) => EntryLotusJson {
                receiver: e.receiver.into(),
                method_num: e.method_num,
            },
            Entry::V16(e) => EntryLotusJson {
                receiver: e.receiver.into(),
                method_num: e.method_num,
            },
            Entry::V17(e) => EntryLotusJson {
                receiver: e.receiver.into(),
                method_num: e.method_num,
            },
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        Entry::V16(fil_actor_cron_state::v16::Entry {
            receiver: lotus_json.receiver.into(),
            method_num: lotus_json.method_num,
        })
    }
}
