// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod shared;

/// Integration tests
#[derive(Debug, clap::Subcommand)]
pub enum TestsCommand {
    #[command(subcommand)]
    Calibnet(shared::TestCommand),
    /// Run the wallet/mpool integration suite against a local devnet. The tests
    /// themselves are chain-agnostic, so the same suite is reused.
    #[command(subcommand)]
    Devnet(shared::TestCommand),
}

impl TestsCommand {
    pub async fn run(self) -> anyhow::Result<()> {
        match self {
            Self::Calibnet(cmd) | Self::Devnet(cmd) => cmd.run().await,
        }
    }
}
