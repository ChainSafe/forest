// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use super::*;
use crate::shim::actors::cron::Entry;
use crate::shim::address::Address;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
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
            Entry::default_latest_version(Address::new_id(0).into(), 10),
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        macro_rules! convert_entry {
            ($($version:ident),+) => {
                match self {
                    $(
                        Entry::$version(e) => EntryLotusJson {
                            receiver: e.receiver.into(),
                            method_num: e.method_num,
                        },
                    )+
                }
            };
        }

        convert_entry!(V8, V9, V10, V11, V12, V13, V14, V15, V16, V17)
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        Entry::default_latest_version(lotus_json.receiver.into(), lotus_json.method_num)
    }
}
crate::test_snapshots!(Entry);
