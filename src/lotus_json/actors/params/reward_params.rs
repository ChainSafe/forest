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

// FIP-0118 (actors v19+) reward stream management. Field names follow the Go structs in
// go-state-types `builtin/v19/reward/{streams,reward_types}.go`.

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

// One version-scoped module covering every FIP-0118 stream type; they were introduced
// together and change together.
macro_rules! impl_reward_stream_params {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_reward_stream_params_ $version>] {
                    use super::*;
                    use fil_actor_reward_state::[<v $version>] as ver;

                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<ver::WeightRecord>();
                        crate::lotus_json::assert_all_snapshots::<ver::WeightRecordUpdate>();
                        crate::lotus_json::assert_all_snapshots::<ver::RecipientShare>();
                        crate::lotus_json::assert_all_snapshots::<ver::DistributionInit>();
                        crate::lotus_json::assert_all_snapshots::<ver::PendingWriteOp>();
                        crate::lotus_json::assert_all_snapshots::<ver::SetWeightRecordsParams>();
                        crate::lotus_json::assert_all_snapshots::<ver::RegisterStreamParams>();
                        crate::lotus_json::assert_all_snapshots::<ver::RemoveStreamParams>();
                        crate::lotus_json::assert_all_snapshots::<ver::SetDistributionParams>();
                        crate::lotus_json::assert_all_snapshots::<ver::SetSharesParams>();
                        crate::lotus_json::assert_all_snapshots::<ver::CancelPendingParams>();
                        crate::lotus_json::assert_all_snapshots::<ver::ClaimParams>();
                    }

                    #[cfg(test)]
                    fn weight_record() -> ver::WeightRecord {
                        ver::WeightRecord { v_start: 95, slope: -1, t_start: 10, floor: 50, cap: 95 }
                    }
                    #[cfg(test)]
                    fn weight_record_json() -> serde_json::Value {
                        json!({ "VStart": 95, "Slope": -1, "TStart": 10, "Floor": 50, "Cap": 95 })
                    }
                    #[cfg(test)]
                    fn share() -> ver::RecipientShare {
                        ver::RecipientShare { recipient: Address::new_id(1235).into(), share: 100 }
                    }
                    #[cfg(test)]
                    fn share_json() -> serde_json::Value {
                        json!({ "Recipient": "f01235", "Share": 100 })
                    }

                    impl HasLotusJson for ver::WeightRecord {
                        type LotusJson = WeightRecordLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(weight_record_json(), weight_record())]
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

                    impl HasLotusJson for ver::WeightRecordUpdate {
                        type LotusJson = WeightRecordUpdateLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(
                                json!({ "ID": 1, "Weight": weight_record_json() }),
                                Self { id: 1, weight: weight_record() },
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

                    impl HasLotusJson for ver::RecipientShare {
                        type LotusJson = RecipientShareLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(share_json(), share())]
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

                    impl HasLotusJson for ver::DistributionInit {
                        type LotusJson = DistributionInitLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![
                                (
                                    json!({ "Writer": "f01234", "Shares": [share_json()] }),
                                    Self { writer: Address::new_id(1234).into(), shares: vec![share()] },
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

                    impl HasLotusJson for ver::PendingWriteOp {
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

                    // Also covers `StepWeightRecordsParams`, an alias of the same type.
                    impl HasLotusJson for ver::SetWeightRecordsParams {
                        type LotusJson = SetWeightRecordsParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![
                                (
                                    json!({ "Updates": [{ "ID": 1, "Weight": weight_record_json() }] }),
                                    Self { updates: vec![ver::WeightRecordUpdate { id: 1, weight: weight_record() }] },
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

                    impl HasLotusJson for ver::RegisterStreamParams {
                        type LotusJson = RegisterStreamParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![
                                (
                                    json!({
                                        "ID": 2,
                                        "Weight": weight_record_json(),
                                        "Distribution": { "Writer": "f01234", "Shares": [share_json()] },
                                        "ActivationEpoch": 100
                                    }),
                                    Self {
                                        id: 2,
                                        weight: weight_record(),
                                        distribution: Some(ver::DistributionInit {
                                            writer: Address::new_id(1234).into(),
                                            shares: vec![share()],
                                        }),
                                        activation_epoch: 100,
                                    },
                                ),
                                (
                                    json!({
                                        "ID": 1,
                                        "Weight": weight_record_json(),
                                        "Distribution": null,
                                        "ActivationEpoch": 100
                                    }),
                                    Self { id: 1, weight: weight_record(), distribution: None, activation_epoch: 100 },
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

                    impl HasLotusJson for ver::RemoveStreamParams {
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

                    impl HasLotusJson for ver::SetDistributionParams {
                        type LotusJson = SetDistributionParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(
                                json!({ "ID": 2, "Writer": "f01234" }),
                                Self { id: 2, writer: Address::new_id(1234).into() },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            SetDistributionParamsLotusJson { id: self.id, writer: self.writer.into() }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self { id: lotus_json.id, writer: lotus_json.writer.into() }
                        }
                    }

                    impl HasLotusJson for ver::SetSharesParams {
                        type LotusJson = SetSharesParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![
                                (
                                    json!({ "ID": 2, "Shares": [share_json()] }),
                                    Self { id: 2, shares: vec![share()] },
                                ),
                                (json!({ "ID": 2, "Shares": null }), Self { id: 2, shares: vec![] }),
                            ]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            SetSharesParamsLotusJson { id: self.id, shares: shares_into_lotus_json(self.shares) }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self { id: lotus_json.id, shares: shares_from_lotus_json(lotus_json.shares) }
                        }
                    }

                    impl HasLotusJson for ver::CancelPendingParams {
                        type LotusJson = CancelPendingParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![
                                (
                                    json!({ "ID": 2, "Op": 3 }),
                                    Self { id: Some(2), op: ver::PendingWriteOp::RemoveStream },
                                ),
                                (
                                    json!({ "ID": null, "Op": 0 }),
                                    Self { id: None, op: ver::PendingWriteOp::SetWeightRecords },
                                ),
                            ]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            CancelPendingParamsLotusJson { id: self.id, op: self.op.into_lotus_json() }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self { id: lotus_json.id, op: HasLotusJson::from_lotus_json(lotus_json.op) }
                        }
                    }

                    impl HasLotusJson for ver::ClaimParams {
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

impl_reward_constructor_params!(fvm_shared4::bigint: 19, 18, 17, 16, 15, 14, 13, 12);
impl_reward_constructor_params!(fvm_shared3::bigint: 11);
impl_award_block_reward_params!(19, 18, 17, 16, 15, 14, 13, 12, 11, 10, 9, 8);
impl_update_network_kpi_params!(fvm_shared4::bigint: 19, 18, 17, 16, 15, 14, 13, 12);
impl_update_network_kpi_params!(fvm_shared3::bigint: 11);
impl_reward_stream_params!(19);
