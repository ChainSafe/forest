// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use clap::Subcommand;
use forest_encoding::tuple::*;
use fvm_shared::{clock::ChainEpoch, econ::TokenAmount};

use super::Config;

#[derive(Serialize_tuple, Deserialize_tuple, Clone, Debug)]
struct VestingSchedule {
    entries: Vec<VestingScheduleEntry>,
}

#[derive(Serialize_tuple, Deserialize_tuple, Clone, Debug)]
struct VestingScheduleEntry {
    epoch: ChainEpoch,
    amount: TokenAmount,
}

#[derive(Debug, Subcommand)]
pub enum StateCommands {}

impl StateCommands {
    pub fn run(&self, _config: Config) -> anyhow::Result<()> {
        // match self {}
        Ok(())
    }
}
