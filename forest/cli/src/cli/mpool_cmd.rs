// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::Config;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub enum MpoolCommands {}

impl MpoolCommands {
    pub async fn run(&self, _config: Config) -> anyhow::Result<()> {
        // match self {}
        Ok(())
    }
}
