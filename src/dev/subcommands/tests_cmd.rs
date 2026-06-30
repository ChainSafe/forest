// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod calibnet;

/// Integration tests
#[derive(Debug, clap::Subcommand)]
pub enum TestsCommand {
    #[command(subcommand)]
    Calibnet(calibnet::CalibnetTestsCommand),
    /// Run the wallet/mpool integration suite against a local devnet. The tests
    /// themselves are chain-agnostic, so the calibnet suite is reused.
    #[command(subcommand)]
    Devnet(calibnet::CalibnetTestsCommand),
}

impl TestsCommand {
    pub async fn run(self) -> anyhow::Result<()> {
        match self {
            Self::Calibnet(cmd) | Self::Devnet(cmd) => cmd.run().await,
        }
    }
}
