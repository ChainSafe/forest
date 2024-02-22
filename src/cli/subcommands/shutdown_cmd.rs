// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc_client::ApiInfo;

use crate::cli::subcommands::prompt_confirm;

#[derive(Debug, clap::Args)]
pub struct ShutdownCommand {
    /// Assume "yes" as answer to shutdown prompt
    #[arg(long)]
    force: bool,
}

impl ShutdownCommand {
    pub async fn run(self, api: ApiInfo) -> anyhow::Result<()> {
        println!("Shutting down Forest node");
        if !self.force && !prompt_confirm() {
            println!("Aborted.");
            return Ok(());
        }
        api.shutdown().await?;
        Ok(())
    }
}
