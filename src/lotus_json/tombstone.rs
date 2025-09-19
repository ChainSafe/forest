// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::actors::evm::TombstoneState;
use fvm_shared4::ActorID;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
#[schemars(rename = "Tombstone")]
pub struct TombstoneLotusJson {
    pub origin: ActorID,
    pub nonce: u64,
}

impl HasLotusJson for TombstoneState {
    type LotusJson = TombstoneLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!({
                "Origin": 0,
                "Nonce": 0,
            }),
            TombstoneState::default_latest_version(Default::default(), 0),
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        macro_rules! convert_tombstone {
            ($($version:ident),+) => {
                match self {
                    $(
                        TombstoneState::$version(state) => TombstoneLotusJson {
                            origin: state.origin,
                            nonce: state.nonce,
                        },
                    )+
                }
            };
        }

        convert_tombstone!(V10, V11, V12, V13, V14, V15, V16, V17)
    }

    // Always return the latest version when deserializing
    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        TombstoneState::default_latest_version(lotus_json.origin, lotus_json.nonce)
    }
}
