// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc_client::ApiInfo;

use super::handle_rpc_err;
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
        api.shutdown().await.map_err(handle_rpc_err)?;
        Ok(())
    }
}
