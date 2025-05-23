// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::actors::evm::{State, TombstoneState};
use ::cid::Cid;
use fil_actor_evm_state::v16::{BytecodeHash, Tombstone, TransientData};

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
#[schemars(rename = "EVMState")]
pub struct EVMStateLotusJson {
    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    pub bytecode: Cid,
    #[schemars(with = "LotusJson<BytecodeHash>")]
    #[serde(with = "crate::lotus_json")]
    pub bytecode_hash: BytecodeHash,

    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    pub contract_state: Cid,

    #[schemars(with = "LotusJson<Option<TransientData>>")]
    #[serde(with = "crate::lotus_json", skip_serializing_if = "Option::is_none")]
    pub transient_data: Option<Option<TransientData>>, // only available in evm actor state v16

    pub nonce: u64,

    #[schemars(with = "LotusJson<Option<TombstoneState>>")]
    #[serde(with = "crate::lotus_json")]
    pub tombstone: Option<TombstoneState>,
}

macro_rules! common_evm_state_fields {
    ($state:expr, $version:ident) => {{
        let data: [u8; 32] = $state.bytecode_hash.into();
        EVMStateLotusJson {
            bytecode: $state.bytecode.clone(),
            bytecode_hash: BytecodeHash::from(data),
            contract_state: $state.contract_state.clone(),
            nonce: $state.nonce,
            tombstone: $state.tombstone.map(|t| TombstoneState::$version(t)),
            transient_data: None,
        }
    }};
}

macro_rules! impl_evm_state_lotus_json {
    ($($version:ident),*) => {
        impl HasLotusJson for State {
            type LotusJson = EVMStateLotusJson;

            #[cfg(test)]
            fn snapshots() -> Vec<(serde_json::Value, Self)> {
                vec![]
            }

             fn into_lotus_json(self) -> Self::LotusJson {
                match self {
                    State::V16(state) => {
                        EVMStateLotusJson {
                            transient_data: Option::from(state.transient_data),
                            ..common_evm_state_fields!(state, V16)
                        }
                    },
                    $(
                    State::$version(state) => {
                        EVMStateLotusJson {
                            ..common_evm_state_fields!(state, $version)
                        }
                    },
                    )*
                }
             }

            fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                State::V16(fil_actor_evm_state::v16::State{
                    bytecode: lotus_json.bytecode,
                    bytecode_hash: lotus_json.bytecode_hash.into(),
                    contract_state: lotus_json.contract_state,
                    transient_data: lotus_json.transient_data.unwrap_or_default(),
                    nonce: lotus_json.nonce,
                    tombstone: lotus_json.tombstone.map(|t| match t {
                        TombstoneState::V16(t) => t,
                        _ => {
                            let lotus_entry = t.into_lotus_json();
                            Tombstone {
                                origin: lotus_entry.orign,
                                nonce: lotus_entry.nonce,
                            }
                        }
                    }),
                })
            }
        }
    };
}

impl_evm_state_lotus_json!(V15, V14, V13, V12, V11, V10);
