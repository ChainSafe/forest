// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use super::*;
use crate::shim::actors::cron::{Entry, State};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
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
            // Create a test cron state with some entries
            State::V16(fil_actor_cron_state::v16::State {
                entries: vec![
                    fil_actor_cron_state::v16::Entry {
                        receiver: Address::new_id(1).into(),
                        method_num: 2,
                    },
                    fil_actor_cron_state::v16::Entry {
                        receiver: Address::new_id(2).into(),
                        method_num: 3,
                    },
                ],
            }),
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        match self {
            State::V8(s) => CronStateLotusJson {
                entries: s.entries.into_iter().map(Entry::V8).collect(),
            },
            State::V9(s) => CronStateLotusJson {
                entries: s.entries.into_iter().map(Entry::V9).collect(),
            },
            State::V10(s) => CronStateLotusJson {
                entries: s.entries.into_iter().map(Entry::V10).collect(),
            },
            State::V11(s) => CronStateLotusJson {
                entries: s.entries.into_iter().map(Entry::V11).collect(),
            },
            State::V12(s) => CronStateLotusJson {
                entries: s.entries.into_iter().map(Entry::V12).collect(),
            },
            State::V13(s) => CronStateLotusJson {
                entries: s.entries.into_iter().map(Entry::V13).collect(),
            },
            State::V14(s) => CronStateLotusJson {
                entries: s.entries.into_iter().map(Entry::V14).collect(),
            },
            State::V15(s) => CronStateLotusJson {
                entries: s.entries.into_iter().map(Entry::V15).collect(),
            },
            State::V16(s) => CronStateLotusJson {
                entries: s.entries.into_iter().map(Entry::V16).collect(),
            },
            State::V17(s) => CronStateLotusJson {
                entries: s.entries.into_iter().map(Entry::V17).collect(),
            },
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        // Default to the latest version (V16)
        State::V16(fil_actor_cron_state::v16::State {
            entries: lotus_json
                .entries
                .into_iter()
                .map(|entry| match entry {
                    Entry::V16(e) => e,
                    _ => {
                        let lotus_entry = entry.into_lotus_json();
                        fil_actor_cron_state::v16::Entry {
                            receiver: lotus_entry.receiver.into(),
                            method_num: lotus_entry.method_num,
                        }
                    }
                })
                .collect(),
        })
    }
}
