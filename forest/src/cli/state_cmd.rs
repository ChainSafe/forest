// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub enum StateCommands {}

impl StateCommands {
    pub async fn run(&self) {}
}
