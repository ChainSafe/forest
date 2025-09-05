// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use super::*;
use crate::shim::actors::miner::State;
use crate::shim::econ::TokenAmount;
use ::cid::Cid;
use fil_actor_miner_state::v16::VestingFunds as VestingFundsV16;
use fil_actor_miner_state::v17::VestingFunds as VestingFundsV17;
use fil_actors_shared::fvm_ipld_bitfield::BitField;
use fvm_shared4::clock::ChainEpoch;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
#[schemars(rename = "MinerState")]
pub struct MinerStateLotusJson {
    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    pub info: Cid,

    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub pre_commit_deposits: TokenAmount,

    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub locked_funds: TokenAmount,

    #[schemars(with = "LotusJson<Option<serde_json::Value>>")]
    #[serde(rename = "VestingFunds")]
    pub vesting_funds: Option<VestingFundsValue>,

    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub fee_debt: TokenAmount,

    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub initial_pledge: TokenAmount,

    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    pub pre_committed_sectors: Cid,

    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json", rename = "PreCommittedSectorsCleanUp")]
    pub pre_committed_sectors_cleanup: Cid,

    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    pub allocated_sectors: Cid,

    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    pub sectors: Cid,
    pub proving_period_start: ChainEpoch,

    pub current_deadline: u64,

    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    pub deadlines: Cid,

    #[schemars(with = "LotusJson<BitField>")]
    #[serde(with = "crate::lotus_json")]
    pub early_terminations: BitField,

    pub deadline_cron_active: bool,
}

// VestingFunds can be either a VestingFunds for V17, V16 or a Cid for older versions.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum VestingFundsValue {
    #[schemars(with = "LotusJson<VestingFundsV17>")]
    #[serde(with = "crate::lotus_json")]
    V17(Option<VestingFundsV17>),
    #[schemars(with = "LotusJson<VestingFundsV16>")]
    #[serde(with = "crate::lotus_json")]
    V16(Option<VestingFundsV16>),
    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    Legacy(Cid),
}

macro_rules! common_miner_state_fields {
    ($state:expr) => {{
        MinerStateLotusJson {
            info: $state.info,
            pre_commit_deposits: $state.pre_commit_deposits.clone().into(),
            locked_funds: $state.locked_funds.clone().into(),
            fee_debt: $state.fee_debt.clone().into(),
            initial_pledge: $state.initial_pledge.clone().into(),
            pre_committed_sectors: $state.pre_committed_sectors,
            pre_committed_sectors_cleanup: $state.pre_committed_sectors_cleanup,
            allocated_sectors: $state.allocated_sectors,
            sectors: $state.sectors,
            proving_period_start: $state.proving_period_start,
            current_deadline: $state.current_deadline,
            deadlines: $state.deadlines,
            early_terminations: $state.early_terminations.clone(),
            deadline_cron_active: $state.deadline_cron_active,
            vesting_funds: None, // Will be set separately
        }
    }};
}

// Define the macro to implement HasLotusJson for different State versions
macro_rules! impl_miner_lotus_json {
    ($($version:ident),*) => {
        impl HasLotusJson for State {
            type LotusJson = MinerStateLotusJson;

            #[cfg(test)]
            fn snapshots() -> Vec<(serde_json::Value, Self)> {
                vec![(
                    json!({
                        "Info": {"/":"baeaaaaa"},
                        "PreCommitDeposits": "1000000000000000000",
                        "LockedFunds": "2000000000000000000",
                        "VestingFunds": {
                            "head": {
                                "epoch": 0,
                                "amount": "0"
                            },
                            "tail": {"/":"baeaaaaa"}
                        },
                        "FeeDebt": "400000000000000000",
                        "InitialPledge": "5000000000000000000",
                        "PreCommittedSectors": {"/":"baeaaaaa"},
                        "PreCommittedSectorsCleanup": {"/":"baeaaaaa"},
                        "AllocatedSectors": {"/":"baeaaaaa"},
                        "Sectors": {"/":"baeaaaaa"},
                        "ProvingPeriodStart": 0,
                        "CurrentDeadline": 0,
                        "Deadlines": {"/":"baeaaaaa"},
                        "EarlyTerminations": [0],
                        "DeadlineCronActive": false
                    }),
                    State::V16(fil_actor_miner_state::v16::State {
                        info: Default::default(),
                        pre_commit_deposits: Default::default(),
                        locked_funds: Default::default(),
                        vesting_funds: Default::default(),
                        fee_debt: Default::default(),
                        initial_pledge: Default::default(),
                        pre_committed_sectors: Default::default(),
                        pre_committed_sectors_cleanup: Default::default(),
                        allocated_sectors: Default::default(),
                        sectors: Default::default(),
                        proving_period_start: 0,
                        current_deadline: 0,
                        deadlines: Default::default(),
                        early_terminations: Default::default(),
                        deadline_cron_active: false,
                    }),
                )]
            }

            fn into_lotus_json(self) -> Self::LotusJson {
                match &self {
                    State::V17(state) => {
                        let mut result = common_miner_state_fields!(state);
                        result.vesting_funds = state.vesting_funds.0.as_ref().map(|_|
                            VestingFundsValue::V17(Some(state.vesting_funds.clone())));
                        result
                    }
                    State::V16(state) => {
                        let mut result = common_miner_state_fields!(state);
                        result.vesting_funds = state.vesting_funds.0.as_ref().map(|_|
                            VestingFundsValue::V16(Some(state.vesting_funds.clone())));
                        result
                    }
                    $(
                    State::$version(state) => {
                        let mut result = common_miner_state_fields!(state);
                        result.vesting_funds = Some(VestingFundsValue::Legacy(state.vesting_funds));
                        result
                    },
                    )*
                }
            }

            fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                let vesting_funds = match &lotus_json.vesting_funds {
                    Some(VestingFundsValue::V16(funds)) => funds.clone().unwrap_or_default(),
                    _ => Default::default(),
                };

                // Default to latest version (V16) when deserializing
                State::V16(fil_actor_miner_state::v16::State {
                    info: lotus_json.info,
                    pre_commit_deposits: lotus_json.pre_commit_deposits.into(),
                    locked_funds: lotus_json.locked_funds.into(),
                    vesting_funds,
                    fee_debt: lotus_json.fee_debt.into(),
                    initial_pledge: lotus_json.initial_pledge.into(),
                    pre_committed_sectors: lotus_json.pre_committed_sectors,
                    pre_committed_sectors_cleanup: lotus_json.pre_committed_sectors_cleanup,
                    allocated_sectors: lotus_json.allocated_sectors,
                    sectors: lotus_json.sectors,
                    proving_period_start: lotus_json.proving_period_start,
                    current_deadline: lotus_json.current_deadline,
                    deadlines: lotus_json.deadlines,
                    early_terminations: lotus_json.early_terminations.clone(),
                    deadline_cron_active: lotus_json.deadline_cron_active,
                })
            }
        }
    };
}

// Use the macro for all supported versions, v16 is hardcoded as the latest
impl_miner_lotus_json!(V15, V14, V13, V12, V11, V10, V9, V8);
