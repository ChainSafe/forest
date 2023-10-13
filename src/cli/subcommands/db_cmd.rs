// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::cli_shared::cli::Config;

use clap::Subcommand;

use crate::utils::bail_moved_cmd;

#[derive(Debug, Subcommand)]
pub enum DBCommands {
    // Those subcommands are hidden and only here to help users migrating to forest-tool
    #[command(hide = true)]
    Stats,
    #[command(hide = true)]
    Clean {
        #[arg(long)]
        force: bool,
    },
    // This is a noop as the manual GC is no longer available.
    GC,
}

impl DBCommands {
    pub async fn run(self, _config: &Config) -> anyhow::Result<()> {
        match self {
            Self::Stats => bail_moved_cmd("db stats", "forest-tool db stats"),
            Self::Clean { .. } => bail_moved_cmd("db clean", "forest-tool db destroy"),
            Self::GC => anyhow::bail!("manual garbage collection has been deprecated"),
        }
    }
}
