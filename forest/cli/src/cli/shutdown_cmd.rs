// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_rpc_client::common_shutdown;

use super::{handle_rpc_err, Config};
use crate::cli::prompt_confirm;

#[derive(Debug, clap::Args)]
pub struct ShutdownCommand {
    /// Assume "yes" as answer to shutdown prompt
    #[arg(long, short)]
    yes: bool,
}

impl ShutdownCommand {
    pub async fn run(&self, config: Config) -> anyhow::Result<()> {
        println!("Shutting down Forest node");
        if !self.yes && !prompt_confirm() {
            println!("Aborted.");
            return Ok(());
        }
        common_shutdown((), &config.client.rpc_token)
            .await
            .map_err(handle_rpc_err)?;
        Ok(())
    }
}
