// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use clap::Subcommand;

#[derive(Debug, Subcommand)]
pub enum DBCommands {
    // This is a noop as the manual GC is no longer available.
    #[command(hide = true)]
    GC,
}

impl DBCommands {
    pub async fn run(self) -> anyhow::Result<()> {
        match self {
            Self::GC => anyhow::bail!("manual garbage collection has been deprecated"),
        }
    }
}
