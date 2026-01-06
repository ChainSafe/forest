// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::actors::cron::{Entry, State};
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct CronStateLotusJson {
    #[schemars(with = "LotusJson<Vec<Entry>>")]
    #[serde(with = "crate::lotus_json")]
    pub entries: Vec<Entry>,
}

impl HasLotusJson for State {
    type LotusJson = CronStateLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        use crate::shim::address::Address;
        vec![(
            json!({
                "Entries": [
                    {
                        "Receiver": "f01",
                        "MethodNum": 2
                    },
                    {
                        "Receiver": "f02",
                        "MethodNum": 3
                    }
                ]
            }),
            State::default_latest_version(vec![
                fil_actor_cron_state::v17::Entry {
                    receiver: Address::new_id(1).into(),
                    method_num: 2,
                },
                fil_actor_cron_state::v17::Entry {
                    receiver: Address::new_id(2).into(),
                    method_num: 3,
                },
            ]),
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        macro_rules! convert_cron_state {
            ($($version:ident),+) => {
                match self {
                    $(
                        State::$version(s) => CronStateLotusJson {
                            entries: s.entries.into_iter().map(Entry::$version).collect(),
                        },
                    )+
                }
            };
        }

        convert_cron_state!(V8, V9, V10, V11, V12, V13, V14, V15, V16, V17)
    }

    // Always return the latest version when deserializing
    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        let entries = lotus_json
            .entries
            .into_iter()
            .map(|entry| {
                let lotus_entry = entry.into_lotus_json();
                Entry::default_latest_version(lotus_entry.receiver.into(), lotus_entry.method_num)
            })
            .collect();

        State::default_latest_version_from_entries(entries)
    }
}
crate::test_snapshots!(State);
