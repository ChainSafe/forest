// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::actors::miner::State;
use crate::shim::econ::TokenAmount;
use ::cid::Cid;
use fil_actors_shared::fvm_ipld_bitfield::BitField;
use fvm_shared4::clock::ChainEpoch;

use super::vesting_funds::{VestingFundLotusJson, VestingFundsLotusJson};

#[derive(Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
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

// VestingFunds can be either embedded (V16+) or referenced via Cid (V8-V15).
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum VestingFundsValue {
    Embedded(VestingFundsLotusJson),
    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    Cid(Cid),
}

// Common field handling macro for all miner state versions
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
            vesting_funds: None, // Will be set by specific version handlers
        }
    }};
}

// Embedded VestingFunds handling (V16+) - VestingFunds stored as embedded struct
macro_rules! embedded_vesting_funds_handler {
    ($state:expr) => {{
        let mut result = common_miner_state_fields!($state);
        result.vesting_funds = match &$state.vesting_funds.0 {
            Some(inner) => Some(VestingFundsValue::Embedded(VestingFundsLotusJson {
                head: VestingFundLotusJson {
                    epoch: inner.head.epoch,
                    amount: inner.head.amount.clone().into(),
                },
                tail: inner.tail,
            })),
            None => None,
        };
        result
    }};
}

// CID VestingFunds handling (V8-V15) - VestingFunds stored as Cid reference
macro_rules! cid_vesting_funds_handler {
    ($state:expr) => {{
        let mut result = common_miner_state_fields!($state);
        result.vesting_funds = Some(VestingFundsValue::Cid($state.vesting_funds));
        result
    }};
}

impl HasLotusJson for State {
    type LotusJson = MinerStateLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!({
                "Info": {"/":"baeaaaaa"},
                "PreCommitDeposits": "0",
                "LockedFunds": "0",
                "VestingFunds": null,
                "FeeDebt": "0",
                "InitialPledge": "0",
                "PreCommittedSectors": {"/":"baeaaaaa"},
                "PreCommittedSectorsCleanUp": {"/":"baeaaaaa"},
                "AllocatedSectors": {"/":"baeaaaaa"},
                "Sectors": {"/":"baeaaaaa"},
                "ProvingPeriodStart": 0,
                "CurrentDeadline": 0,
                "Deadlines": {"/":"baeaaaaa"},
                "EarlyTerminations": [0],
                "DeadlineCronActive": false
            }),
            State::default_latest_version(
                Default::default(),
                Default::default(),
                Default::default(),
                fil_actor_miner_state::v17::VestingFunds(None),
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                0,
                0,
                Default::default(),
                Default::default(),
                false,
            ),
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        macro_rules! convert_miner_state {
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

        convert_miner_state! {
            cid_vesting_funds_handler for [V8, V9, V10, V11, V12, V13, V14, V15];
            embedded_vesting_funds_handler for [V16, V17];
        }
    }

    // Always return the latest version when deserializing
    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        let vesting_funds = match &lotus_json.vesting_funds {
            Some(VestingFundsValue::Embedded(vesting_funds_json)) => {
                use fil_actor_miner_state::v17::{VestingFund, VestingFunds, VestingFundsInner};
                VestingFunds(Some(VestingFundsInner {
                    head: VestingFund {
                        epoch: vesting_funds_json.head.epoch,
                        amount: vesting_funds_json.head.amount.clone().into(),
                    },
                    tail: vesting_funds_json.tail,
                }))
            }
            Some(VestingFundsValue::Cid(_)) => {
                use fil_actor_miner_state::v17::VestingFunds;
                VestingFunds(None)
            }
            None => {
                use fil_actor_miner_state::v17::VestingFunds;
                VestingFunds(None)
            }
        };

        State::default_latest_version(
            lotus_json.info,
            lotus_json.pre_commit_deposits.into(),
            lotus_json.locked_funds.into(),
            vesting_funds,
            lotus_json.fee_debt.into(),
            lotus_json.initial_pledge.into(),
            lotus_json.pre_committed_sectors,
            lotus_json.pre_committed_sectors_cleanup,
            lotus_json.allocated_sectors,
            lotus_json.sectors,
            lotus_json.proving_period_start,
            lotus_json.current_deadline,
            lotus_json.deadlines,
            lotus_json.early_terminations.clone(),
            lotus_json.deadline_cron_active,
        )
    }
}
crate::test_snapshots!(State);
