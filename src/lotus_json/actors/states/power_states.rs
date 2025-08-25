// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use super::*;
use crate::shim::actors::power::State;
use crate::shim::clock::ChainEpoch;
use crate::shim::econ::TokenAmount;
use ::cid::Cid;
use fil_actors_shared::v16::reward::FilterEstimate;
use num::BigInt;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
#[schemars(rename = "PowerState")]
pub struct PowerStateLotusJson {
    #[schemars(with = "LotusJson<BigInt>")]
    #[serde(with = "crate::lotus_json")]
    pub total_raw_byte_power: BigInt,

    #[schemars(with = "LotusJson<BigInt>")]
    #[serde(with = "crate::lotus_json")]
    pub total_bytes_committed: BigInt,

    #[schemars(with = "LotusJson<BigInt>")]
    #[serde(with = "crate::lotus_json")]
    pub total_quality_adj_power: BigInt,

    #[schemars(with = "LotusJson<BigInt>")]
    #[serde(with = "crate::lotus_json", rename = "TotalQABytesCommitted")]
    pub total_qa_bytes_committed: BigInt,

    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub total_pledge_collateral: TokenAmount,

    #[schemars(with = "LotusJson<BigInt>")]
    #[serde(with = "crate::lotus_json")]
    pub this_epoch_raw_byte_power: BigInt,

    #[schemars(with = "LotusJson<BigInt>")]
    #[serde(with = "crate::lotus_json")]
    pub this_epoch_quality_adj_power: BigInt,
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub this_epoch_pledge_collateral: TokenAmount,

    #[schemars(with = "LotusJson<FilterEstimate>")]
    #[serde(with = "crate::lotus_json", rename = "ThisEpochQAPowerSmoothed")]
    pub this_epoch_qa_power_smoothed: FilterEstimate,

    pub miner_count: i64,
    pub miner_above_min_power_count: i64,

    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    pub cron_event_queue: Cid,

    #[schemars(with = "LotusJson<ChainEpoch>")]
    #[serde(with = "crate::lotus_json")]
    pub first_cron_epoch: ChainEpoch,

    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    pub claims: Cid,

    #[schemars(with = "LotusJson<Option<Cid>>")]
    #[serde(with = "crate::lotus_json")]
    pub proof_validation_batch: Option<Cid>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub ramp_start_epoch: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ramp_duration_epochs: Option<u64>,
}

macro_rules! common_power_state_fields {
    ($state:expr) => {{
        PowerStateLotusJson {
            total_raw_byte_power: $state.total_raw_byte_power,
            total_bytes_committed: $state.total_bytes_committed,
            total_quality_adj_power: $state.total_quality_adj_power,
            total_qa_bytes_committed: $state.total_qa_bytes_committed,
            total_pledge_collateral: $state.total_pledge_collateral.into(),
            this_epoch_raw_byte_power: $state.this_epoch_raw_byte_power,
            this_epoch_quality_adj_power: $state.this_epoch_quality_adj_power,
            this_epoch_pledge_collateral: $state.this_epoch_pledge_collateral.into(),
            this_epoch_qa_power_smoothed: FilterEstimate {
                position: $state.this_epoch_qa_power_smoothed.position,
                velocity: $state.this_epoch_qa_power_smoothed.velocity,
            },
            miner_count: $state.miner_count,
            miner_above_min_power_count: $state.miner_above_min_power_count,
            cron_event_queue: $state.cron_event_queue,
            first_cron_epoch: $state.first_cron_epoch,
            claims: $state.claims,
            proof_validation_batch: $state.proof_validation_batch,

            ramp_start_epoch: None,
            ramp_duration_epochs: None,
        }
    }};
}

macro_rules! v15_to_v17_power_state_fields {
    ($state:expr) => {{
        PowerStateLotusJson {
            ramp_start_epoch: Some($state.ramp_start_epoch),
            ramp_duration_epochs: Some($state.ramp_duration_epochs),
            ..common_power_state_fields!($state)
        }
    }};
}

macro_rules! v8_to_v14_power_state_fields {
    ($state:expr) => {{
        PowerStateLotusJson {
            ..common_power_state_fields!($state)
        }
    }};
}

macro_rules! implement_state_versions {
    (
        $(
            $handler:ident for [ $( $version:ident ),+ ]
        );* $(;)?
    ) => {
        impl HasLotusJson for State {
            type LotusJson = PowerStateLotusJson;

            #[cfg(test)]
            fn snapshots() -> Vec<(serde_json::Value, Self)> {
                todo!()
            }

            fn into_lotus_json(self) -> Self::LotusJson {
                match self {
                    $(
                        $(
                            State::$version(state) => $handler!(state),
                        )+
                    )*
                }
            }

            fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                State::V16(fil_actor_power_state::v16::State {
                    total_raw_byte_power: lotus_json.total_raw_byte_power,
                    total_bytes_committed: lotus_json.total_bytes_committed,
                    total_quality_adj_power: lotus_json.total_quality_adj_power,
                    total_qa_bytes_committed: lotus_json.total_qa_bytes_committed,
                    total_pledge_collateral: lotus_json.total_pledge_collateral.into(),
                    this_epoch_raw_byte_power: lotus_json.this_epoch_raw_byte_power,
                    this_epoch_quality_adj_power: lotus_json.this_epoch_quality_adj_power,
                    this_epoch_pledge_collateral: lotus_json.this_epoch_pledge_collateral.into(),
                    this_epoch_qa_power_smoothed: lotus_json.this_epoch_qa_power_smoothed,
                    miner_count: lotus_json.miner_count,
                    miner_above_min_power_count: lotus_json.miner_above_min_power_count,
                    cron_event_queue: lotus_json.cron_event_queue,
                    first_cron_epoch: lotus_json.first_cron_epoch,
                    claims: lotus_json.claims,
                    proof_validation_batch: lotus_json.proof_validation_batch,
                    ramp_start_epoch: lotus_json.ramp_start_epoch.unwrap_or(0),
                    ramp_duration_epochs: lotus_json.ramp_duration_epochs.unwrap_or(0),
                })
            }
        }
    };
}

implement_state_versions! {
    v8_to_v14_power_state_fields for [V8, V9, V10, V11, V12, V13, V14];
    v15_to_v17_power_state_fields for [V15, V16, V17];
}
