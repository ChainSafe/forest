// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use pastey::paste;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
#[schemars(rename = "BytecodeHash")]
pub struct BytecodeHashLotusJson([u8; 32]);

macro_rules! impl_bytecode_hash_lotus_json {
    ($($version:literal),+) => {
        $(
        paste! {
            impl HasLotusJson for fil_actor_evm_state::[<v $version>]::BytecodeHash {
                type LotusJson = BytecodeHashLotusJson;

                #[cfg(test)]
                fn snapshots() -> Vec<(serde_json::Value, Self)> {
                    vec![]
                }

                fn into_lotus_json(self) -> Self::LotusJson {
                    BytecodeHashLotusJson(self.into())
                }

                fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                    Self::from(lotus_json.0)
                }
            }
        }
        )+
    };
}

impl_bytecode_hash_lotus_json!(10, 11, 12, 13, 14, 15, 16, 17);
