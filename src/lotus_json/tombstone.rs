// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::actors::evm::TombstoneState;
use fvm_shared4::ActorID;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
#[schemars(rename = "Tombstone")]
pub struct TombstoneLotusJson {
    pub orign: ActorID,
    pub nonce: u64,
}

macro_rules! impl_tombstone_lotus_json {
    ($($version:ident),*) => {
        impl HasLotusJson for TombstoneState {
            type LotusJson = TombstoneLotusJson;

            #[cfg(test)]
            fn snapshots() -> Vec<(serde_json::Value, Self)> {
                vec![(
                    json!({
                        "Origin": 0,
                        "Nonce": 0,
                    }),
                    TombstoneState::V16(fil_actor_evm_state::v16::Tombstone {
                        origin: 0,
                        nonce: 0,
                    })
                    )]
            }

            fn into_lotus_json(self) -> Self::LotusJson {
                match self {
                    $(
                    TombstoneState::$version(state) => TombstoneLotusJson {
                        orign: state.origin,
                        nonce: state.nonce,
                    },)*
                }
            }

            fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                TombstoneState::V16(fil_actor_evm_state::v16::Tombstone {
                    origin: lotus_json.orign,
                    nonce: lotus_json.nonce,
                })
            }
        }
    };
}

impl_tombstone_lotus_json!(V16, V15, V14, V13, V12, V11, V10);
