// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod fees;
pub use fees::*;

use clap::Subcommand;

/// Miner related commands.
#[derive(Debug, Subcommand)]
pub enum MinerCommands {
    Fees(FeesCommand),
}

impl MinerCommands {
    pub async fn run(self, client: crate::rpc::Client) -> anyhow::Result<()> {
        match self {
            Self::Fees(cmd) => cmd.run(client).await,
        }
    }
}
