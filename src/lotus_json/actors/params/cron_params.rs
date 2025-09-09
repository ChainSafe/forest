// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::actors::cron::Entry;
use paste::paste;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, JsonSchema, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct CronConstructorParamsLotusJson {
    #[schemars(with = "LotusJson<Vec<Entry>>")]
    #[serde(with = "crate::lotus_json")]
    pub entries: Vec<Entry>,
}

macro_rules! impl_lotus_json_for_cron_constructor_params {
    ($($version:literal),+) => {
        $(
            paste! {
                impl HasLotusJson for fil_actor_cron_state::[<v $version>]::ConstructorParams {
                    type LotusJson = CronConstructorParamsLotusJson;

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
                            Self {
                                entries: vec![
                                    fil_actor_cron_state::[<v $version>]::Entry {
                                        receiver: Address::new_id(1).into(),
                                        method_num: 2,
                                    },
                                    fil_actor_cron_state::[<v $version>]::Entry {
                                        receiver: Address::new_id(2).into(),
                                        method_num: 3,
                                    },
                                ],
                            },
                        )]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        Self::LotusJson {
                            entries: self.entries.into_iter().map(Entry::[<V $version>]).collect(),
                        }
                    }

                    fn from_lotus_json(json: Self::LotusJson) -> Self {
                        Self {
                            entries: json.entries.into_iter().map(|entry| match entry {
                                Entry::[<V $version>](e) => e,
                                _ => {
                                    let lotus_entry = entry.into_lotus_json();
                                    fil_actor_cron_state::[<v $version>]::Entry {
                                        receiver: lotus_entry.receiver.into(),
                                        method_num: lotus_entry.method_num,
                                    }
                                }
                            }).collect(),
                        }
                    }
                }
            }
        )+
    };
}

impl_lotus_json_for_cron_constructor_params!(8, 9, 10, 11, 12, 13, 14, 15, 16, 17);
