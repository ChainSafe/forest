// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::address::Address;
use crate::shim::clock::ChainEpoch;
use crate::shim::econ::TokenAmount;
use crate::shim::message::MethodNum;
use fvm_ipld_encoding::RawBytes;
use pastey::paste;
use serde::{Deserialize, Serialize};

// ConstructorParams
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ConstructorParamsLotusJson {
    #[schemars(with = "LotusJson<Vec<Address>>")]
    #[serde(with = "crate::lotus_json")]
    pub signers: Vec<Address>,

    pub num_approvals_threshold: u64,

    #[schemars(with = "LotusJson<ChainEpoch>")]
    #[serde(with = "crate::lotus_json")]
    pub unlock_duration: ChainEpoch,

    #[schemars(with = "LotusJson<ChainEpoch>")]
    #[serde(with = "crate::lotus_json")]
    pub start_epoch: ChainEpoch,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ProposeParamsLotusJson {
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub to: Address,

    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub value: TokenAmount,

    #[schemars(with = "LotusJson<MethodNum>")]
    #[serde(with = "crate::lotus_json")]
    pub method: MethodNum,

    #[schemars(with = "LotusJson<RawBytes>")]
    #[serde(with = "crate::lotus_json")]
    pub params: RawBytes,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct TxnIDParamsLotusJson {
    #[schemars(with = "i64")]
    #[serde(rename = "ID")]
    pub id: i64,

    #[schemars(with = "LotusJson<Vec<u8>>")]
    #[serde(with = "crate::lotus_json")]
    pub proposal_hash: Vec<u8>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct AddSignerParamsLotusJson {
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub signer: Address,

    pub increase: bool,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct RemoveSignerParamsLotusJson {
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub signer: Address,

    pub decrease: bool,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct SwapSignerParamsLotusJson {
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub from: Address,

    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub to: Address,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ChangeNumApprovalsThresholdParamsLotusJson {
    pub new_threshold: u64,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct LockBalanceParamsLotusJson {
    #[schemars(with = "LotusJson<ChainEpoch>")]
    #[serde(with = "crate::lotus_json")]
    pub start_epoch: ChainEpoch,

    #[schemars(with = "LotusJson<ChainEpoch>")]
    #[serde(with = "crate::lotus_json")]
    pub unlock_duration: ChainEpoch,

    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub amount: TokenAmount,
}

// Macro to implement HasLotusJson for ConstructorParams across all versions
macro_rules! impl_multisig_constructor_params {
    ($($version:literal),+) => {
        $(
            paste! {
                impl HasLotusJson for fil_actor_multisig_state::[<v $version>]::ConstructorParams {
                    type LotusJson = ConstructorParamsLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![(
                            json!({
                                "Signers": ["f01234", "f01235"],
                                "NumApprovalsThreshold": 2,
                                "UnlockDuration": 100,
                                "StartEpoch": 0,
                            }),
                            Self {
                                signers: vec![Address::new_id(1234).into(), Address::new_id(1235).into()],
                                num_approvals_threshold: 2,
                                unlock_duration: 100,
                                start_epoch: 0,
                            },
                        )]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        ConstructorParamsLotusJson {
                            signers: self.signers.into_iter().map(|a| a.into()).collect(),
                            num_approvals_threshold: self.num_approvals_threshold,
                            unlock_duration: self.unlock_duration,
                            start_epoch: self.start_epoch,
                        }
                    }

                    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                        Self {
                            signers: lotus_json.signers.into_iter().map(|a| a.into()).collect(),
                            num_approvals_threshold: lotus_json.num_approvals_threshold,
                            unlock_duration: lotus_json.unlock_duration,
                            start_epoch: lotus_json.start_epoch,
                        }
                    }
                }
            }
        )+
    };
}

// Macro to implement HasLotusJson for ProposeParams across all versions
macro_rules! impl_multisig_propose_params {
    ($($version:literal),+) => {
        $(
            paste! {
                impl HasLotusJson for fil_actor_multisig_state::[<v $version>]::ProposeParams {
                    type LotusJson = ProposeParamsLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![(
                            json!({
                                "To": "f01234",
                                "Value": "1000000000000000000",
                                "Method": 0,
                                "Params": "Ynl0ZSBhcnJheQ==",
                            }),
                            Self {
                                to: Address::new_id(1234).into(),
                                value: TokenAmount::from_atto(1000000000000000000u64).into(),
                                method: 0,
                                params: RawBytes::new(b"byte array".to_vec()),
                            },
                        )]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        ProposeParamsLotusJson {
                            to: self.to.into(),
                            value: self.value.into(),
                            method: self.method,
                            params: self.params,
                        }
                    }

                    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                        Self {
                            to: lotus_json.to.into(),
                            value: lotus_json.value.into(),
                            method: lotus_json.method,
                            params: lotus_json.params,
                        }
                    }
                }
            }
        )+
    };
}

// Macro to implement HasLotusJson for TxnIDParams across all versions
macro_rules! impl_multisig_txn_id_params {
    ($($version:literal),+) => {
        $(
            paste! {
                impl HasLotusJson for fil_actor_multisig_state::[<v $version>]::TxnIDParams {
                    type LotusJson = TxnIDParamsLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![(
                            json!({
                                "ID": 1234,
                                "ProposalHash": "YWJjZGVmZ2g=",
                            }),
                            Self {
                                id: fil_actor_multisig_state::[<v $version>]::TxnID(1234),
                                proposal_hash: b"abcdefgh".to_vec(),
                            },
                        )]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        TxnIDParamsLotusJson {
                            id: self.id.0,
                            proposal_hash: self.proposal_hash,
                        }
                    }

                    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                        Self {
                            id: fil_actor_multisig_state::[<v $version>]::TxnID(lotus_json.id),
                            proposal_hash: lotus_json.proposal_hash,
                        }
                    }
                }
            }
        )+
    };
}

// Macro to implement HasLotusJson for AddSignerParams across all versions
macro_rules! impl_multisig_add_signer_params {
    ($($version:literal),+) => {
        $(
            paste! {
                impl HasLotusJson for fil_actor_multisig_state::[<v $version>]::AddSignerParams {
                    type LotusJson = AddSignerParamsLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![(
                            json!({
                                "Signer": "f01234",
                                "Increase": true,
                            }),
                            Self {
                                signer: Address::new_id(1234).into(),
                                increase: true,
                            },
                        )]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        AddSignerParamsLotusJson {
                            signer: self.signer.into(),
                            increase: self.increase,
                        }
                    }

                    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                        Self {
                            signer: lotus_json.signer.into(),
                            increase: lotus_json.increase,
                        }
                    }
                }
            }
        )+
    };
}

// Macro to implement HasLotusJson for RemoveSignerParams across all versions
macro_rules! impl_multisig_remove_signer_params {
    ($($version:literal),+) => {
        $(
            paste! {
                impl HasLotusJson for fil_actor_multisig_state::[<v $version>]::RemoveSignerParams {
                    type LotusJson = RemoveSignerParamsLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![(
                            json!({
                                "Signer": "f01234",
                                "Decrease": false,
                            }),
                            Self {
                                signer: Address::new_id(1234).into(),
                                decrease: false,
                            },
                        )]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        RemoveSignerParamsLotusJson {
                            signer: self.signer.into(),
                            decrease: self.decrease,
                        }
                    }

                    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                        Self {
                            signer: lotus_json.signer.into(),
                            decrease: lotus_json.decrease,
                        }
                    }
                }
            }
        )+
    };
}

// Macro to implement HasLotusJson for SwapSignerParams across all versions
macro_rules! impl_multisig_swap_signer_params {
    ($($version:literal),+) => {
        $(
            paste! {
                impl HasLotusJson for fil_actor_multisig_state::[<v $version>]::SwapSignerParams {
                    type LotusJson = SwapSignerParamsLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![(
                            json!({
                                "From": "f01234",
                                "To": "f01235",
                            }),
                            Self {
                                from: Address::new_id(1234).into(),
                                to: Address::new_id(1235).into(),
                            },
                        )]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        SwapSignerParamsLotusJson {
                            from: self.from.into(),
                            to: self.to.into(),
                        }
                    }

                    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                        Self {
                            from: lotus_json.from.into(),
                            to: lotus_json.to.into(),
                        }
                    }
                }
            }
        )+
    };
}

// Macro to implement HasLotusJson for ChangeNumApprovalsThresholdParams across all versions
macro_rules! impl_multisig_change_num_approvals_threshold_params {
    ($($version:literal),+) => {
        $(
            paste! {
                impl HasLotusJson for fil_actor_multisig_state::[<v $version>]::ChangeNumApprovalsThresholdParams {
                    type LotusJson = ChangeNumApprovalsThresholdParamsLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![(
                            json!({
                                "NewThreshold": 3,
                            }),
                            Self { new_threshold: 3 },
                        )]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        ChangeNumApprovalsThresholdParamsLotusJson {
                            new_threshold: self.new_threshold,
                        }
                    }

                    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                        Self {
                            new_threshold: lotus_json.new_threshold,
                        }
                    }
                }
            }
        )+
    };
}

// Macro to implement HasLotusJson for LockBalanceParams across all versions
macro_rules! impl_multisig_lock_balance_params {
    ($($version:literal),+) => {
        $(
            paste! {
                impl HasLotusJson for fil_actor_multisig_state::[<v $version>]::LockBalanceParams {
                    type LotusJson = LockBalanceParamsLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![(
                            json!({
                                "StartEpoch": 100,
                                "UnlockDuration": 200,
                                "Amount": "5000000000000000000",
                            }),
                            Self {
                                start_epoch: 100,
                                unlock_duration: 200,
                                amount: TokenAmount::from_atto(5000000000000000000u64).into(),
                            },
                        )]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        LockBalanceParamsLotusJson {
                            start_epoch: self.start_epoch,
                            unlock_duration: self.unlock_duration,
                            amount: self.amount.into(),
                        }
                    }

                    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                        Self {
                            start_epoch: lotus_json.start_epoch,
                            unlock_duration: lotus_json.unlock_duration,
                            amount: lotus_json.amount.into(),
                        }
                    }
                }
            }
        )+
    };
}

impl_multisig_constructor_params!(8, 9, 10, 11, 12, 13, 14, 15, 16, 17);
impl_multisig_propose_params!(8, 9, 10, 11, 12, 13, 14, 15, 16, 17);
impl_multisig_txn_id_params!(8, 9, 10, 11, 12, 13, 14, 15, 16, 17);
impl_multisig_add_signer_params!(8, 9, 10, 11, 12, 13, 14, 15, 16, 17);
impl_multisig_remove_signer_params!(8, 9, 10, 11, 12, 13, 14, 15, 16, 17);
impl_multisig_swap_signer_params!(8, 9, 10, 11, 12, 13, 14, 15, 16, 17);
impl_multisig_change_num_approvals_threshold_params!(8, 9, 10, 11, 12, 13, 14, 15, 16, 17);
impl_multisig_lock_balance_params!(8, 9, 10, 11, 12, 13, 14, 15, 16, 17);
