// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::{self, RpcMethodExt, chain::ChainPruneSnapshot};
use clap::Subcommand;
use std::time::Duration;

#[derive(Debug, Subcommand)]
pub enum ChainPruneCommands {
    /// Run snapshot GC
    Snap,
}

impl ChainPruneCommands {
    pub async fn run(self, client: rpc::Client) -> anyhow::Result<()> {
        match self {
            Self::Snap => {
                client
                    .call(ChainPruneSnapshot::request(())?.with_timeout(Duration::MAX))
                    .await?;
            }
        }

        Ok(())
    }
}
