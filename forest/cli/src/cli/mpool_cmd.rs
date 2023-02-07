// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use structopt::StructOpt;

use super::Config;

#[derive(Debug, StructOpt)]
pub enum MpoolCommands {}

impl MpoolCommands {
    pub fn run(&self, _config: Config) -> anyhow::Result<()> {
        // match self {}
        Ok(())
    }
}
