// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::address::Address;
use crate::shim::clock::ChainEpoch;
use crate::shim::econ::TokenAmount;
use fvm_ipld_encoding::repr::{Deserialize_repr, Serialize_repr};
use num_bigint::BigInt;
use pastey::paste;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(transparent)]
pub struct RewardConstructorParamsLotusJson(
    #[schemars(with = "LotusJson<Option<BigInt>>")]
    #[serde(with = "crate::lotus_json")]
    Option<BigInt>,
);

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct AwardBlockRewardParamsLotusJson {
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub miner: Address,
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub penalty: TokenAmount,
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub gas_reward: TokenAmount,
    pub win_count: i64,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(transparent)]
pub struct UpdateNetworkKPIParamsLotusJson(
    #[schemars(with = "LotusJson<Option<BigInt>>")]
    #[serde(with = "crate::lotus_json")]
    Option<BigInt>,
);

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct WeightRecordLotusJson {
    pub v_start: u64,
    pub slope: i64,
    pub t_start: ChainEpoch,
    pub floor: u64,
    pub cap: u64,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct WeightRecordUpdateLotusJson {
    #[serde(rename = "ID")]
    pub id: u64,
    pub weight: WeightRecordLotusJson,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct RecipientShareLotusJson {
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub recipient: Address,
    pub share: u64,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct DistributionInitLotusJson {
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub writer: Address,
    // Lotus returns null (not []) for an empty share list; None means empty.
    pub shares: Option<Vec<RecipientShareLotusJson>>,
}

#[derive(Serialize_repr, Deserialize_repr, JsonSchema, Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
#[schemars(with = "u8")]
pub enum PendingWriteOpLotusJson {
    SetWeightRecords = 0,
    StepWeightRecords = 1,
    RegisterStream = 2,
    RemoveStream = 3,
    SetDistribution = 4,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct SetWeightRecordsParamsLotusJson {
    pub updates: Option<Vec<WeightRecordUpdateLotusJson>>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct RegisterStreamParamsLotusJson {
    #[serde(rename = "ID")]
    pub id: u64,
    pub weight: WeightRecordLotusJson,
    pub distribution: Option<DistributionInitLotusJson>,
    pub activation_epoch: ChainEpoch,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct RemoveStreamParamsLotusJson {
    #[serde(rename = "ID")]
    pub id: u64,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct SetDistributionParamsLotusJson {
    #[serde(rename = "ID")]
    pub id: u64,
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub writer: Address,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct SetSharesParamsLotusJson {
    #[serde(rename = "ID")]
    pub id: u64,
    pub shares: Option<Vec<RecipientShareLotusJson>>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct CancelPendingParamsLotusJson {
    #[serde(rename = "ID")]
    pub id: Option<u64>,
    pub op: PendingWriteOpLotusJson,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ClaimParamsLotusJson {
    #[serde(rename = "ID")]
    pub id: u64,
    #[schemars(with = "LotusJson<Vec<Address>>")]
    #[serde(with = "crate::lotus_json")]
    pub wallets: Vec<Address>,
}

fn shares_into_lotus_json<T: HasLotusJson<LotusJson = RecipientShareLotusJson>>(
    shares: Vec<T>,
) -> Option<Vec<RecipientShareLotusJson>> {
    if shares.is_empty() {
        None
    } else {
        Some(shares.into_iter().map(T::into_lotus_json).collect())
    }
}

fn shares_from_lotus_json<T: HasLotusJson<LotusJson = RecipientShareLotusJson>>(
    shares: Option<Vec<RecipientShareLotusJson>>,
) -> Vec<T> {
    shares
        .unwrap_or_default()
        .into_iter()
        .map(T::from_lotus_json)
        .collect()
}

// Implementation for ConstructorParams
macro_rules! impl_reward_constructor_params {
    ($type_suffix:path: $($version:literal),+) => {
        $(
            paste! {
                mod [<impl_reward_constructor_params_ $version>] {
                    use super::*;
                    type T = fil_actor_reward_state::[<v $version>]::ConstructorParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = RewardConstructorParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![
                                (
                                    json!(null),
                                    Self {
                                        power: None,
                                    },
                                ),
                                (
                                    json!("1000"),
                                    Self {
                                        power: Some($type_suffix::bigint_ser::BigIntDe(BigInt::from(1000))),
                                    },
                                ),
                            ]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            RewardConstructorParamsLotusJson(self.power.map(|p| p.0))
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                power: lotus_json.0.map(|p| $type_suffix::bigint_ser::BigIntDe(p)),
                            }
                        }
                    }
                }
            }
        )+
    };
}

// Implementation for AwardBlockRewardParams
macro_rules! impl_award_block_reward_params {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_award_block_reward_params_ $version>] {
                    use super::*;
                    type T = fil_actor_reward_state::[<v $version>]::AwardBlockRewardParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = AwardBlockRewardParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![
                                (
                                    json!({
                                        "Miner": "f01234",
                                        "Penalty": "0",
                                        "GasReward": "1000",
                                        "WinCount": 1
                                    }),
                                    Self {
                                        miner: Address::new_id(1234).into(),
                                        penalty: TokenAmount::from_atto(0).into(),
                                        gas_reward: TokenAmount::from_atto(1000).into(),
                                        win_count: 1,
                                    },
                                ),
                            ]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            AwardBlockRewardParamsLotusJson {
                                miner: self.miner.into(),
                                penalty: self.penalty.into(),
                                gas_reward: self.gas_reward.into(),
                                win_count: self.win_count,
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                miner: lotus_json.miner.into(),
                                penalty: TokenAmount::from(lotus_json.penalty).into(),
                                gas_reward: TokenAmount::from(lotus_json.gas_reward).into(),
                                win_count: lotus_json.win_count,
                            }
                        }
                    }
                }
            }
        )+
    };
}

// Implementation for UpdateNetworkKPIParams
macro_rules! impl_update_network_kpi_params {
    ($type_suffix:path: $($version:literal),+) => {
        $(
            paste! {
                mod [<impl_update_network_kpi_params_ $version>] {
                    use super::*;
                    type T = fil_actor_reward_state::[<v $version>]::UpdateNetworkKPIParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = UpdateNetworkKPIParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![
                                (
                                    json!(null),
                                    Self {
                                        curr_realized_power: None,
                                    },
                                ),
                                (
                                    json!("2000"),
                                    Self {
                                        curr_realized_power: Some($type_suffix::bigint_ser::BigIntDe(BigInt::from(2000))),
                                    },
                                ),
                            ]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            UpdateNetworkKPIParamsLotusJson(self.curr_realized_power.map(|p| p.0))
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                curr_realized_power: lotus_json.0.map(|p| $type_suffix::bigint_ser::BigIntDe(p)),
                            }
                        }
                    }
                }
            }
        )+
    };
}

macro_rules! impl_reward_weight_record {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_reward_weight_record_ $version>] {
                    use super::*;
                    type T = fil_actor_reward_state::[<v $version>]::WeightRecord;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = WeightRecordLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(
                                json!({ "VStart": 95, "Slope": -1, "TStart": 10, "Floor": 50, "Cap": 95 }),
                                Self { v_start: 95, slope: -1, t_start: 10, floor: 50, cap: 95 },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            WeightRecordLotusJson {
                                v_start: self.v_start,
                                slope: self.slope,
                                t_start: self.t_start,
                                floor: self.floor,
                                cap: self.cap,
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                v_start: lotus_json.v_start,
                                slope: lotus_json.slope,
                                t_start: lotus_json.t_start,
                                floor: lotus_json.floor,
                                cap: lotus_json.cap,
                            }
                        }
                    }
                }
            }
        )+
    };
}

macro_rules! impl_reward_weight_record_update {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_reward_weight_record_update_ $version>] {
                    use super::*;
                    type T = fil_actor_reward_state::[<v $version>]::WeightRecordUpdate;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = WeightRecordUpdateLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(
                                json!({
                                    "ID": 1,
                                    "Weight": { "VStart": 95, "Slope": -1, "TStart": 10, "Floor": 50, "Cap": 95 }
                                }),
                                Self {
                                    id: 1,
                                    weight: fil_actor_reward_state::[<v $version>]::WeightRecord {
                                        v_start: 95,
                                        slope: -1,
                                        t_start: 10,
                                        floor: 50,
                                        cap: 95,
                                    },
                                },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            WeightRecordUpdateLotusJson {
                                id: self.id,
                                weight: self.weight.into_lotus_json(),
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                id: lotus_json.id,
                                weight: HasLotusJson::from_lotus_json(lotus_json.weight),
                            }
                        }
                    }
                }
            }
        )+
    };
}

macro_rules! impl_reward_recipient_share {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_reward_recipient_share_ $version>] {
                    use super::*;
                    type T = fil_actor_reward_state::[<v $version>]::RecipientShare;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = RecipientShareLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(
                                json!({ "Recipient": "f01235", "Share": 100 }),
                                Self { recipient: Address::new_id(1235).into(), share: 100 },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            RecipientShareLotusJson {
                                recipient: self.recipient.into(),
                                share: self.share,
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                recipient: lotus_json.recipient.into(),
                                share: lotus_json.share,
                            }
                        }
                    }
                }
            }
        )+
    };
}

macro_rules! impl_reward_distribution_init {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_reward_distribution_init_ $version>] {
                    use super::*;
                    type T = fil_actor_reward_state::[<v $version>]::DistributionInit;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = DistributionInitLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![
                                (
                                    json!({
                                        "Writer": "f01234",
                                        "Shares": [{ "Recipient": "f01235", "Share": 100 }]
                                    }),
                                    Self {
                                        writer: Address::new_id(1234).into(),
                                        shares: vec![fil_actor_reward_state::[<v $version>]::RecipientShare {
                                            recipient: Address::new_id(1235).into(),
                                            share: 100,
                                        }],
                                    },
                                ),
                                (
                                    json!({ "Writer": "f01234", "Shares": null }),
                                    Self { writer: Address::new_id(1234).into(), shares: vec![] },
                                ),
                            ]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            DistributionInitLotusJson {
                                writer: self.writer.into(),
                                shares: shares_into_lotus_json(self.shares),
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                writer: lotus_json.writer.into(),
                                shares: shares_from_lotus_json(lotus_json.shares),
                            }
                        }
                    }
                }
            }
        )+
    };
}

macro_rules! impl_reward_pending_write_op {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_reward_pending_write_op_ $version>] {
                    use super::*;
                    type T = fil_actor_reward_state::[<v $version>]::PendingWriteOp;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = PendingWriteOpLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![
                                (json!(0), Self::SetWeightRecords),
                                (json!(1), Self::StepWeightRecords),
                                (json!(2), Self::RegisterStream),
                                (json!(3), Self::RemoveStream),
                                (json!(4), Self::SetDistribution),
                            ]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            match self {
                                Self::SetWeightRecords => PendingWriteOpLotusJson::SetWeightRecords,
                                Self::StepWeightRecords => PendingWriteOpLotusJson::StepWeightRecords,
                                Self::RegisterStream => PendingWriteOpLotusJson::RegisterStream,
                                Self::RemoveStream => PendingWriteOpLotusJson::RemoveStream,
                                Self::SetDistribution => PendingWriteOpLotusJson::SetDistribution,
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            match lotus_json {
                                PendingWriteOpLotusJson::SetWeightRecords => Self::SetWeightRecords,
                                PendingWriteOpLotusJson::StepWeightRecords => Self::StepWeightRecords,
                                PendingWriteOpLotusJson::RegisterStream => Self::RegisterStream,
                                PendingWriteOpLotusJson::RemoveStream => Self::RemoveStream,
                                PendingWriteOpLotusJson::SetDistribution => Self::SetDistribution,
                            }
                        }
                    }
                }
            }
        )+
    };
}

// `StepWeightRecordsParams` is an alias of this type, so it is covered too.
macro_rules! impl_set_weight_records_params {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_set_weight_records_params_ $version>] {
                    use super::*;
                    type T = fil_actor_reward_state::[<v $version>]::SetWeightRecordsParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = SetWeightRecordsParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![
                                (
                                    json!({
                                        "Updates": [{
                                            "ID": 1,
                                            "Weight": { "VStart": 95, "Slope": -1, "TStart": 10, "Floor": 50, "Cap": 95 }
                                        }]
                                    }),
                                    Self {
                                        updates: vec![fil_actor_reward_state::[<v $version>]::WeightRecordUpdate {
                                            id: 1,
                                            weight: fil_actor_reward_state::[<v $version>]::WeightRecord {
                                                v_start: 95,
                                                slope: -1,
                                                t_start: 10,
                                                floor: 50,
                                                cap: 95,
                                            },
                                        }],
                                    },
                                ),
                                (json!({ "Updates": null }), Self { updates: vec![] }),
                            ]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            SetWeightRecordsParamsLotusJson {
                                updates: if self.updates.is_empty() {
                                    None
                                } else {
                                    Some(self.updates.into_iter().map(|u| u.into_lotus_json()).collect())
                                },
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                updates: lotus_json
                                    .updates
                                    .unwrap_or_default()
                                    .into_iter()
                                    .map(HasLotusJson::from_lotus_json)
                                    .collect(),
                            }
                        }
                    }
                }
            }
        )+
    };
}

macro_rules! impl_register_stream_params {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_register_stream_params_ $version>] {
                    use super::*;
                    type T = fil_actor_reward_state::[<v $version>]::RegisterStreamParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = RegisterStreamParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![
                                (
                                    json!({
                                        "ID": 2,
                                        "Weight": { "VStart": 95, "Slope": -1, "TStart": 10, "Floor": 50, "Cap": 95 },
                                        "Distribution": {
                                            "Writer": "f01234",
                                            "Shares": [{ "Recipient": "f01235", "Share": 100 }]
                                        },
                                        "ActivationEpoch": 100
                                    }),
                                    Self {
                                        id: 2,
                                        weight: fil_actor_reward_state::[<v $version>]::WeightRecord {
                                            v_start: 95,
                                            slope: -1,
                                            t_start: 10,
                                            floor: 50,
                                            cap: 95,
                                        },
                                        distribution: Some(fil_actor_reward_state::[<v $version>]::DistributionInit {
                                            writer: Address::new_id(1234).into(),
                                            shares: vec![fil_actor_reward_state::[<v $version>]::RecipientShare {
                                                recipient: Address::new_id(1235).into(),
                                                share: 100,
                                            }],
                                        }),
                                        activation_epoch: 100,
                                    },
                                ),
                                (
                                    json!({
                                        "ID": 1,
                                        "Weight": { "VStart": 95, "Slope": -1, "TStart": 10, "Floor": 50, "Cap": 95 },
                                        "Distribution": null,
                                        "ActivationEpoch": 100
                                    }),
                                    Self {
                                        id: 1,
                                        weight: fil_actor_reward_state::[<v $version>]::WeightRecord {
                                            v_start: 95,
                                            slope: -1,
                                            t_start: 10,
                                            floor: 50,
                                            cap: 95,
                                        },
                                        distribution: None,
                                        activation_epoch: 100,
                                    },
                                ),
                            ]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            RegisterStreamParamsLotusJson {
                                id: self.id,
                                weight: self.weight.into_lotus_json(),
                                distribution: self.distribution.map(|d| d.into_lotus_json()),
                                activation_epoch: self.activation_epoch,
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                id: lotus_json.id,
                                weight: HasLotusJson::from_lotus_json(lotus_json.weight),
                                distribution: lotus_json.distribution.map(HasLotusJson::from_lotus_json),
                                activation_epoch: lotus_json.activation_epoch,
                            }
                        }
                    }
                }
            }
        )+
    };
}

macro_rules! impl_remove_stream_params {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_remove_stream_params_ $version>] {
                    use super::*;
                    type T = fil_actor_reward_state::[<v $version>]::RemoveStreamParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = RemoveStreamParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(json!({ "ID": 2 }), Self { id: 2 })]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            RemoveStreamParamsLotusJson { id: self.id }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self { id: lotus_json.id }
                        }
                    }
                }
            }
        )+
    };
}

macro_rules! impl_set_distribution_params {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_set_distribution_params_ $version>] {
                    use super::*;
                    type T = fil_actor_reward_state::[<v $version>]::SetDistributionParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = SetDistributionParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(
                                json!({ "ID": 2, "Writer": "f01234" }),
                                Self { id: 2, writer: Address::new_id(1234).into() },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            SetDistributionParamsLotusJson {
                                id: self.id,
                                writer: self.writer.into(),
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                id: lotus_json.id,
                                writer: lotus_json.writer.into(),
                            }
                        }
                    }
                }
            }
        )+
    };
}

macro_rules! impl_set_shares_params {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_set_shares_params_ $version>] {
                    use super::*;
                    type T = fil_actor_reward_state::[<v $version>]::SetSharesParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = SetSharesParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![
                                (
                                    json!({ "ID": 2, "Shares": [{ "Recipient": "f01235", "Share": 100 }] }),
                                    Self {
                                        id: 2,
                                        shares: vec![fil_actor_reward_state::[<v $version>]::RecipientShare {
                                            recipient: Address::new_id(1235).into(),
                                            share: 100,
                                        }],
                                    },
                                ),
                                (json!({ "ID": 2, "Shares": null }), Self { id: 2, shares: vec![] }),
                            ]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            SetSharesParamsLotusJson {
                                id: self.id,
                                shares: shares_into_lotus_json(self.shares),
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                id: lotus_json.id,
                                shares: shares_from_lotus_json(lotus_json.shares),
                            }
                        }
                    }
                }
            }
        )+
    };
}

macro_rules! impl_cancel_pending_params {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_cancel_pending_params_ $version>] {
                    use super::*;
                    type T = fil_actor_reward_state::[<v $version>]::CancelPendingParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = CancelPendingParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![
                                (
                                    json!({ "ID": 2, "Op": 3 }),
                                    Self {
                                        id: Some(2),
                                        op: fil_actor_reward_state::[<v $version>]::PendingWriteOp::RemoveStream,
                                    },
                                ),
                                (
                                    json!({ "ID": null, "Op": 0 }),
                                    Self {
                                        id: None,
                                        op: fil_actor_reward_state::[<v $version>]::PendingWriteOp::SetWeightRecords,
                                    },
                                ),
                            ]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            CancelPendingParamsLotusJson {
                                id: self.id,
                                op: self.op.into_lotus_json(),
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                id: lotus_json.id,
                                op: HasLotusJson::from_lotus_json(lotus_json.op),
                            }
                        }
                    }
                }
            }
        )+
    };
}

macro_rules! impl_claim_params {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_claim_params_ $version>] {
                    use super::*;
                    type T = fil_actor_reward_state::[<v $version>]::ClaimParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = ClaimParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![
                                (
                                    json!({ "ID": 2, "Wallets": ["f01234", "f01235"] }),
                                    Self {
                                        id: 2,
                                        wallets: vec![Address::new_id(1234).into(), Address::new_id(1235).into()],
                                    },
                                ),
                                (json!({ "ID": 2, "Wallets": null }), Self { id: 2, wallets: vec![] }),
                            ]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            ClaimParamsLotusJson {
                                id: self.id,
                                wallets: self.wallets.into_iter().map(Into::into).collect(),
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                id: lotus_json.id,
                                wallets: lotus_json.wallets.into_iter().map(Into::into).collect(),
                            }
                        }
                    }
                }
            }
        )+
    };
}

impl_reward_constructor_params!(fvm_shared4::bigint: 12, 13, 14, 15, 16, 17, 18, 19);
impl_reward_constructor_params!(fvm_shared3::bigint: 11);
impl_award_block_reward_params!(8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19);
impl_update_network_kpi_params!(fvm_shared4::bigint: 12, 13, 14, 15, 16, 17, 18, 19);
impl_update_network_kpi_params!(fvm_shared3::bigint: 11);
impl_reward_weight_record!(19);
impl_reward_weight_record_update!(19);
impl_reward_recipient_share!(19);
impl_reward_distribution_init!(19);
impl_reward_pending_write_op!(19);
impl_set_weight_records_params!(19);
impl_register_stream_params!(19);
impl_remove_stream_params!(19);
impl_set_distribution_params!(19);
impl_set_shares_params!(19);
impl_cancel_pending_params!(19);
impl_claim_params!(19);
