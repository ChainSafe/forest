// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::cli::subcommands::prompt_confirm;
use crate::rpc::{self, prelude::*};

#[derive(Debug, clap::Args)]
pub struct ShutdownCommand {
    /// Assume "yes" as answer to shutdown prompt
    #[arg(long)]
    force: bool,
}

impl ShutdownCommand {
    pub async fn run(self, client: rpc::Client) -> anyhow::Result<()> {
        println!("Shutting down Forest node");
        if !self.force && !prompt_confirm() {
            println!("Aborted.");
            return Ok(());
        }
        Shutdown::call(&client, ()).await?;
        Ok(())
    }
}
