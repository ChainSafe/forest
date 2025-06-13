// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use super::*;
use crate::shim::address::Address;
use paste::paste;

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct MinerChangeWorkerParamsLotusJson {
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub new_worker: Address,
    #[schemars(with = "LotusJson<Vec<Address>>")]
    #[serde(with = "crate::lotus_json")]
    pub new_control_addresses: Vec<Address>,
}

macro_rules!  impl_lotus_json_for_miner_change_worker_param {
    ($($version:literal),+) => {
        $(
        paste! {
                impl HasLotusJson for fil_actor_miner_state::[<v $version>]::ChangeWorkerAddressParams {
                    type LotusJson = MinerChangeWorkerParamsLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![
                            (
                                json!({
                                    "NewWorker": "f01234",
                                    "NewControlAddrs": ["f01236", "f01237"],
                                }),
                                Self {
                                    new_worker: Address::new_id(1234).into(),
                                    new_control_addresses: vec![Address::new_id(1236).into(), Address::new_id(1237).into()],
                                },
                            ),
                        ]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        MinerChangeWorkerParamsLotusJson {
                            new_worker: self.new_worker.into(),
                            new_control_addresses: self.new_control_addresses
                                .into_iter()
                                .map(|a| a.into())
                                .collect(),
                        }
                    }

                    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                        Self {
                            new_worker: lotus_json.new_worker.into(),
                            new_control_addresses: lotus_json.new_control_addresses
                                .into_iter()
                                .map(|a| a.into())
                                .collect(),
                        }
                    }
                }
            }
        )+
    };
}

impl_lotus_json_for_miner_change_worker_param!(8, 9, 10, 11, 12, 13, 14, 15, 16);
