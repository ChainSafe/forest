// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use ::cid::Cid;
use fvm_shared4::ActorID;
use paste::paste;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
#[schemars(rename = "TransientData")]
pub struct TransientDataLotusJson {
    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    pub transient_data_state: Cid,
    pub transient_data_lifespan: TransientDataLifespanLotusJson,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
#[schemars(rename = "TransientDataLifespan")]
pub struct TransientDataLifespanLotusJson {
    pub origin: ActorID,
    pub nonce: u64,
}

macro_rules! impl_transient_data_lotus_json {
    ($($version:literal),+) => {
        $(
        paste! {
            impl HasLotusJson for fil_actor_evm_state::[<v $version>]::TransientData {
                type LotusJson = TransientDataLotusJson;

                #[cfg(test)]
                fn snapshots() -> Vec<(serde_json::Value, Self)> {
                    vec![(
                        json! {{
                            "TransientDataState": {"/":"baeaaaaa"},
                            "TransientDataLifespan": {
                                "Origin": "2",
                                "Nonce": "3"
                            }
                        }},
                        Self {
                            transient_data_state: Cid::default(),
                            transient_data_lifespan: fil_actor_evm_state::[<v $version>]::TransientDataLifespan {
                                origin: 2,
                                nonce: 3,
                            },
                        },
                    )]
                }

                fn into_lotus_json(self) -> Self::LotusJson {
                    TransientDataLotusJson {
                        transient_data_state: self.transient_data_state,
                        transient_data_lifespan: TransientDataLifespanLotusJson {
                            origin: self.transient_data_lifespan.origin,
                            nonce: self.transient_data_lifespan.nonce,
                        },
                    }
                }

                fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                    Self {
                        transient_data_state: lotus_json.transient_data_state,
                        transient_data_lifespan: fil_actor_evm_state::[<v $version>]::TransientDataLifespan {
                            origin: lotus_json.transient_data_lifespan.origin,
                            nonce: lotus_json.transient_data_lifespan.nonce,
                        },
                    }
                }
            }

            impl HasLotusJson for fil_actor_evm_state::[<v $version>]::TransientDataLifespan {
                type LotusJson = TransientDataLifespanLotusJson;

                #[cfg(test)]
                fn snapshots() -> Vec<(serde_json::Value, Self)> {
                    vec![(
                        json! {{
                            "Origin": 1,
                            "Nonce": 2
                        }},
                        Self {
                            origin: 1,
                            nonce: 2,
                        },
                    )]
                }

                fn into_lotus_json(self) -> Self::LotusJson {
                    TransientDataLifespanLotusJson {
                        origin: self.origin,
                        nonce: self.nonce,
                    }
                }

                fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                    Self {
                        origin: lotus_json.origin,
                        nonce: lotus_json.nonce,
                    }
                }
            }
        }
        )+
    };
}

impl_transient_data_lotus_json!(16, 17);
