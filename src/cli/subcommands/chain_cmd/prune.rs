// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::{self, RpcMethodExt, chain::ChainPruneSnapshot};
use clap::Subcommand;
use std::time::Duration;

/// Prune chain database
#[derive(Debug, Subcommand)]
pub enum ChainPruneCommands {
    /// Run snapshot GC
    Snap {
        /// Do not block until GC is completed
        #[arg(long)]
        no_wait: bool,
    },
}

impl ChainPruneCommands {
    pub async fn run(self, client: rpc::Client) -> anyhow::Result<()> {
        match self {
            Self::Snap { no_wait } => {
                client
                    .call(ChainPruneSnapshot::request((!no_wait,))?.with_timeout(Duration::MAX))
                    .await?;
            }
        }

        Ok(())
    }
}
