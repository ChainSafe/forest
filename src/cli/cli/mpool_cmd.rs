// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use clap::Subcommand;

use super::Config;

#[derive(Debug, Subcommand)]
pub enum MpoolCommands {}

impl MpoolCommands {
    pub fn run(&self, _config: Config) -> anyhow::Result<()> {
        // match self {}
        Ok(())
    }
}
