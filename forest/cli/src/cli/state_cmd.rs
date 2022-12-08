// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::Config;
use forest_encoding::tuple::*;
use fvm_shared::clock::ChainEpoch;
use fvm_shared::econ::TokenAmount;
use structopt::StructOpt;

#[derive(Serialize_tuple, Deserialize_tuple, Clone, Debug)]
struct VestingSchedule {
    entries: Vec<VestingScheduleEntry>,
}

#[derive(Serialize_tuple, Deserialize_tuple, Clone, Debug)]
struct VestingScheduleEntry {
    epoch: ChainEpoch,
    amount: TokenAmount,
}

#[derive(Debug, StructOpt)]
pub enum StateCommands {}

impl StateCommands {
    pub async fn run(&self, _config: Config) {
        // match self {}
    }
}
