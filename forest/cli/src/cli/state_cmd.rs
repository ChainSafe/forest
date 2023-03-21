// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use clap::Subcommand;
use fvm_shared::{clock::ChainEpoch, econ::TokenAmount};
use serde_tuple::{self, Deserialize_tuple, Serialize_tuple};

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
