// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::actors::evm::{State, TombstoneState};
use ::cid::Cid;
use pastey::paste;

#[derive(Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
#[schemars(rename = "EVMState")]
pub struct EVMStateLotusJson {
    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    pub bytecode: Cid,
    pub bytecode_hash: [u8; 32],

    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    pub contract_state: Cid,

    // TransientData is only available in evm actor state v16+
    // None = field doesn't exist (v10-v15)
    // Some(None) = field exists but is None (v16+)
    // Some(Some(data)) = field exists and has data (v16+)
    pub transient_data: Option<Option<transient_data::TransientDataLotusJson>>,

    pub nonce: u64,

    pub tombstone: Option<tombstone::TombstoneLotusJson>,
}

macro_rules! impl_evm_state_lotus_json {
    // Special case for versions without transient_data (v10-v15)
    (no_transient_data: $($version:literal),+) => {
        $(
        paste! {
            mod [<impl_evm_state_lotus_json_ $version>] {
                use super::*;
                type T = fil_actor_evm_state::[<v $version>]::State;
                #[test]
                fn snapshots() {
                    crate::lotus_json::assert_all_snapshots::<T>();
                }
                impl HasLotusJson for T {
                    type LotusJson = EVMStateLotusJson;
                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![(
                            json!({
                                "Bytecode": {"/":"baeaaaaa"},
                                "BytecodeHash": [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
                                "ContractState": {"/":"baeaaaaa"},
                                "Nonce": 0,
                                "Tombstone": null,
                                "TransientData": null
                            }),
                            Self {
                                bytecode: Cid::default(),
                                bytecode_hash: fil_actor_evm_state::[<v $version>]::BytecodeHash::from([0; 32]),
                                contract_state: Cid::default(),
                                nonce: 0,
                                tombstone: None,
                            },
                        )]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        let bytecode_hash_data: [u8; 32] = self.bytecode_hash.into();

                        EVMStateLotusJson {
                            bytecode: self.bytecode,
                            bytecode_hash: bytecode_hash_data,
                            contract_state: self.contract_state,
                            nonce: self.nonce,
                            tombstone: self.tombstone.map(|t| {
                                let tombstone_state = TombstoneState::[<V $version>](t);
                                tombstone_state.into_lotus_json()
                            }),
                            transient_data: None,
                        }
                    }

                    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                        let tombstone = lotus_json.tombstone.map(|tombstone_lotus| {
                            fil_actor_evm_state::[<v $version>]::Tombstone {
                                origin: tombstone_lotus.origin.into(),
                                nonce: tombstone_lotus.nonce,
                            }
                        });

                        Self {
                            bytecode: lotus_json.bytecode,
                            bytecode_hash: fil_actor_evm_state::[<v $version>]::BytecodeHash::from(lotus_json.bytecode_hash),
                            contract_state: lotus_json.contract_state,
                            nonce: lotus_json.nonce,
                            tombstone,
                        }
                    }
                }
            }
        }
        )+
    };
    // Special case for versions with transient_data (v16+)
    (with_transient_data: $($version:literal),+) => {
        $(
        paste! {
            mod [<impl_evm_state_lotus_json_ $version>] {
                use super::*;
                type T = fil_actor_evm_state::[<v $version>]::State;
                #[test]
                fn snapshots() {
                    crate::lotus_json::assert_all_snapshots::<T>();
                }
                impl HasLotusJson for T {
                    type LotusJson = EVMStateLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![(
                            json!({
                                "Bytecode": {"/":"baeaaaaa"},
                                "BytecodeHash": [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
                                "ContractState": {"/":"baeaaaaa"},
                                "Nonce": 0,
                                "Tombstone": null,
                                "TransientData": null
                            }),
                            Self {
                                bytecode: Cid::default(),
                                bytecode_hash: fil_actor_evm_state::[<v $version>]::BytecodeHash::from([0; 32]),
                                contract_state: Cid::default(),
                                nonce: 0,
                                tombstone: None,
                                transient_data: None,
                            },
                        )]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        let bytecode_hash_data: [u8; 32] = self.bytecode_hash.into();

                        EVMStateLotusJson {
                            bytecode: self.bytecode,
                            bytecode_hash: bytecode_hash_data,
                            contract_state: self.contract_state,
                            nonce: self.nonce,
                            tombstone: self.tombstone.map(|t| {
                                let tombstone_state = TombstoneState::[<V $version>](t);
                                tombstone_state.into_lotus_json()
                            }),
                            transient_data: Some(self.transient_data.map(|td| {
                                td.into_lotus_json()
                            })),
                        }
                    }

                    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                        let tombstone = lotus_json.tombstone.map(|tombstone_lotus| {
                            fil_actor_evm_state::[<v $version>]::Tombstone {
                                origin: tombstone_lotus.origin.into(),
                                nonce: tombstone_lotus.nonce,
                            }
                        });

                        let transient_data = lotus_json.transient_data
                            .and_then(|outer_option| outer_option)
                            .map(|transient_data_lotus| {
                                fil_actor_evm_state::[<v $version>]::TransientData::from_lotus_json(transient_data_lotus)
                            });

                        Self {
                            bytecode: lotus_json.bytecode,
                            bytecode_hash: fil_actor_evm_state::[<v $version>]::BytecodeHash::from(lotus_json.bytecode_hash),
                            contract_state: lotus_json.contract_state,
                            nonce: lotus_json.nonce,
                            tombstone,
                            transient_data,
                        }
                    }
                }
            }
        }
        )+
    };
}

impl HasLotusJson for State {
    type LotusJson = EVMStateLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!({
                "Bytecode": {"/":"baeaaaaa"},
                "BytecodeHash": [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
                "ContractState": {"/":"baeaaaaa"},
                "Nonce": 0,
                "Tombstone": null,
                "TransientData": null
            }),
            State::default_latest_version(Cid::default(), [0; 32], Cid::default(), None, 0, None),
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        macro_rules! convert_evm_state {
            ($($version:ident),+) => {
                match self {
                    $(
                        State::$version(state) => state.into_lotus_json(),
                    )+
                }
            };
        }

        convert_evm_state!(V10, V11, V12, V13, V14, V15, V16, V17)
    }

    // Always return the latest version when deserializing
    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        let latest_state = fil_actor_evm_state::v17::State::from_lotus_json(lotus_json);
        State::default_latest_version(
            latest_state.bytecode,
            latest_state.bytecode_hash.into(),
            latest_state.contract_state,
            latest_state.transient_data,
            latest_state.nonce,
            latest_state.tombstone,
        )
    }
}
crate::test_snapshots!(State);

// Implement for versions without transient_data (v10-v15)
impl_evm_state_lotus_json!(no_transient_data: 10, 11, 12, 13, 14, 15);

// Implement for versions with transient_data (v16+)
impl_evm_state_lotus_json!(with_transient_data: 16, 17);
