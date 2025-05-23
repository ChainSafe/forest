// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use ::cid::Cid;
use fil_actor_evm_state::v16::{TransientData, TransientDataLifespan};
use fvm_shared4::ActorID;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
#[schemars(rename = "TransientData")]
pub struct TransientDataLotusJson {
    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    pub transient_data_state: Cid,
    #[schemars(with = "LotusJson<TransientDataLifespan>")]
    #[serde(with = "crate::lotus_json")]
    pub transient_data_lifespan: TransientDataLifespan,
}

impl HasLotusJson for TransientData {
    type LotusJson = TransientDataLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json! {{
                "TransientDataState": "1",
                "TransientDataLifespan": {
                    "Origin": "2",
                    "Nonce": "3"
                }
            }},
            Self {
                transient_data_state: Default::default(),
                transient_data_lifespan: TransientDataLifespan {
                    origin: Default::default(),
                    nonce: Default::default(),
                },
            },
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        TransientDataLotusJson {
            transient_data_state: self.transient_data_state,
            transient_data_lifespan: self.transient_data_lifespan,
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        Self {
            transient_data_state: lotus_json.transient_data_state,
            transient_data_lifespan: lotus_json.transient_data_lifespan,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
#[schemars(rename = "TransientDataLifespan")]
pub struct TransientDataLifespanLotusJson {
    pub origin: ActorID,
    pub nonce: u64,
}

impl HasLotusJson for TransientDataLifespan {
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
