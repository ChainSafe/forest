// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::actors::reward::{State, StreamAccrual};
use crate::shim::address::Address;
use crate::shim::clock::ChainEpoch;
use crate::shim::econ::TokenAmount;
use ::cid::Cid;
use fil_actors_shared::v16::reward::FilterEstimate;
use num_bigint::BigInt;

#[derive(Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
#[schemars(rename = "RewardState")]
pub struct RewardStateLotusJson {
    #[schemars(with = "LotusJson<BigInt>")]
    #[serde(with = "crate::lotus_json")]
    pub cumsum_baseline: BigInt,

    #[schemars(with = "LotusJson<BigInt>")]
    #[serde(with = "crate::lotus_json")]
    pub cumsum_realized: BigInt,

    #[schemars(with = "LotusJson<ChainEpoch>")]
    #[serde(with = "crate::lotus_json")]
    pub effective_network_time: ChainEpoch,

    #[schemars(with = "LotusJson<BigInt>")]
    #[serde(with = "crate::lotus_json")]
    pub effective_baseline_power: BigInt,

    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub this_epoch_reward: TokenAmount,

    #[schemars(with = "LotusJson<FilterEstimate>")]
    #[serde(with = "crate::lotus_json")]
    pub this_epoch_reward_smoothed: FilterEstimate,

    #[schemars(with = "LotusJson<BigInt>")]
    #[serde(with = "crate::lotus_json")]
    pub this_epoch_baseline_power: BigInt,

    #[schemars(with = "LotusJson<ChainEpoch>")]
    #[serde(with = "crate::lotus_json")]
    pub epoch: ChainEpoch,

    // v8 to v18.
    #[schemars(with = "LotusJson<Option<TokenAmount>>")]
    #[serde(
        with = "crate::lotus_json",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub total_storage_power_reward: Option<TokenAmount>,

    // v19 onwards.
    #[schemars(with = "LotusJson<Option<TokenAmount>>")]
    #[serde(
        with = "crate::lotus_json",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub simple_total: Option<TokenAmount>,

    #[schemars(with = "LotusJson<Option<TokenAmount>>")]
    #[serde(
        with = "crate::lotus_json",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub baseline_total: Option<TokenAmount>,

    // v19 onwards (FIP-0118).
    #[schemars(with = "LotusJson<Option<TokenAmount>>")]
    #[serde(
        with = "crate::lotus_json",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub total_minted_reward: Option<TokenAmount>,

    #[schemars(with = "LotusJson<Option<TokenAmount>>")]
    #[serde(
        with = "crate::lotus_json",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub total_burn_minted: Option<TokenAmount>,

    #[schemars(with = "LotusJson<Option<TokenAmount>>")]
    #[serde(
        with = "crate::lotus_json",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub total_explicit_minted: Option<TokenAmount>,

    #[schemars(with = "LotusJson<Option<Vec<StreamAccrual>>>")]
    #[serde(
        with = "crate::lotus_json",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub accrued: Option<Vec<StreamAccrual>>,

    #[schemars(with = "LotusJson<Option<ChainEpoch>>")]
    #[serde(
        with = "crate::lotus_json",
        rename = "SWATimelockEpochs",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub swa_timelock_epochs: Option<ChainEpoch>,

    #[schemars(with = "LotusJson<Option<Address>>")]
    #[serde(
        with = "crate::lotus_json",
        rename = "SWAActor",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub swa_actor: Option<Address>,

    #[schemars(with = "LotusJson<Option<Cid>>")]
    #[serde(
        with = "crate::lotus_json",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub streams_root: Option<Cid>,
}

#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct StreamAccrualLotusJson {
    #[serde(rename = "ID")]
    pub id: u64,

    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub amount: TokenAmount,
}

impl HasLotusJson for StreamAccrual {
    type LotusJson = StreamAccrualLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!({ "ID": 2, "Amount": "1" }),
            Self {
                id: 2,
                amount: TokenAmount::from_atto(1),
            },
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        StreamAccrualLotusJson {
            id: self.id,
            amount: self.amount,
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        Self {
            id: lotus_json.id,
            amount: lotus_json.amount,
        }
    }
}

// Fields shared by every version; the version-specific groups start out absent.
macro_rules! common_reward_state_fields {
    ($state:expr) => {{
        RewardStateLotusJson {
            cumsum_baseline: $state.cumsum_baseline.into(),
            cumsum_realized: $state.cumsum_realized.into(),
            effective_network_time: $state.effective_network_time,
            effective_baseline_power: $state.effective_baseline_power.into(),
            this_epoch_reward: $state.this_epoch_reward.into(),
            this_epoch_reward_smoothed: FilterEstimate {
                position: $state.this_epoch_reward_smoothed.position,
                velocity: $state.this_epoch_reward_smoothed.velocity,
            },
            this_epoch_baseline_power: $state.this_epoch_baseline_power.into(),
            epoch: $state.epoch,
            total_storage_power_reward: None,
            simple_total: None,
            baseline_total: None,
            total_minted_reward: None,
            total_burn_minted: None,
            total_explicit_minted: None,
            accrued: None,
            swa_timelock_epochs: None,
            swa_actor: None,
            streams_root: None,
        }
    }};
}

macro_rules! v8_to_v18_reward_state_fields {
    ($state:expr) => {{
        RewardStateLotusJson {
            total_storage_power_reward: Some($state.total_storage_power_reward.into()),
            simple_total: Some($state.simple_total.into()),
            baseline_total: Some($state.baseline_total.into()),
            ..common_reward_state_fields!($state)
        }
    }};
}

macro_rules! v19_plus_reward_state_fields {
    ($state:expr) => {{
        RewardStateLotusJson {
            total_minted_reward: Some($state.total_minted_reward.into()),
            total_burn_minted: Some($state.total_burn_minted.into()),
            total_explicit_minted: Some($state.total_explicit_minted.into()),
            accrued: Some(
                $state
                    .accrued
                    .into_iter()
                    .map(StreamAccrual::from)
                    .collect(),
            ),
            swa_timelock_epochs: Some($state.swa_timelock_epochs),
            swa_actor: Some($state.swa_actor.into()),
            streams_root: Some($state.streams_root),
            ..common_reward_state_fields!($state)
        }
    }};
}

impl HasLotusJson for State {
    type LotusJson = RewardStateLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!({
                "CumsumBaseline": "1",
                "CumsumRealized": "1",
                "EffectiveNetworkTime": 1,
                "EffectiveBaselinePower": "1",
                "ThisEpochReward": "1",
                "ThisEpochRewardSmoothed": {
                    "PositionEstimate": "1",
                    "VelocityEstimate": "1",
                },
                "ThisEpochBaselinePower": "1",
                "Epoch": 1,
                "TotalMintedReward": "1",
                "TotalBurnMinted": "1",
                "TotalExplicitMinted": "1",
                "Accrued": [{ "ID": 2, "Amount": "1" }],
                "SWATimelockEpochs": 120,
                "SWAActor": "f01234",
                "StreamsRoot": {"/":"baeaaaaa"},
            }),
            State::default_latest_version(
                BigInt::from(1),
                BigInt::from(1),
                1,
                BigInt::from(1),
                TokenAmount::from_atto(1).into(),
                fil_actors_shared::v19::builtin::reward::smooth::FilterEstimate {
                    position: BigInt::from(1),
                    velocity: BigInt::from(1),
                },
                BigInt::from(1),
                1,
                TokenAmount::from_atto(1).into(),
                TokenAmount::from_atto(1).into(),
                TokenAmount::from_atto(1).into(),
                vec![fil_actor_reward_state::v19::StreamAccrual {
                    id: 2,
                    amount: TokenAmount::from_atto(1).into(),
                }],
                120,
                Address::new_id(1234).into(),
                Cid::default(),
            ),
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        macro_rules! convert_reward_state {
            (
                $(
                    $handler:ident for [ $( $version:ident ),+ ]
                );+ $(;)?
            ) => {
                match self {
                    $(
                        $(
                            State::$version(state) => $handler!(state),
                        )+
                    )+
                }
            };
        }

        convert_reward_state! {
            v8_to_v18_reward_state_fields for [V8, V9, V10, V11, V12, V13, V14, V15, V16, V17, V18];
            v19_plus_reward_state_fields for [V19];
        }
    }

    // Always return the latest version when deserializing
    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        State::default_latest_version(
            lotus_json.cumsum_baseline,
            lotus_json.cumsum_realized,
            lotus_json.effective_network_time,
            lotus_json.effective_baseline_power,
            lotus_json.this_epoch_reward.into(),
            fil_actors_shared::v19::builtin::reward::smooth::FilterEstimate {
                position: lotus_json.this_epoch_reward_smoothed.position,
                velocity: lotus_json.this_epoch_reward_smoothed.velocity,
            },
            lotus_json.this_epoch_baseline_power,
            lotus_json.epoch,
            lotus_json.total_minted_reward.unwrap_or_default().into(),
            lotus_json.total_burn_minted.unwrap_or_default().into(),
            lotus_json.total_explicit_minted.unwrap_or_default().into(),
            lotus_json
                .accrued
                .unwrap_or_default()
                .into_iter()
                .map(Into::into)
                .collect(),
            lotus_json.swa_timelock_epochs.unwrap_or_default(),
            lotus_json.swa_actor.unwrap_or_default().into(),
            lotus_json.streams_root.unwrap_or_default(),
        )
    }
}
crate::test_snapshots!(State);
