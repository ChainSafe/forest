// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod helpers;
mod mpool;
mod wallet;

/// Shared integration tests (used by both calibnet and devnet)
#[derive(Debug, clap::Subcommand)]
pub enum TestCommand {
    Wallet(wallet::WalletTestCommand),
    Mpool(mpool::MpoolTestCommand),
}

impl TestCommand {
    pub async fn run(self) -> anyhow::Result<()> {
        match self {
            Self::Wallet(cmd) => cmd.run().await,
            Self::Mpool(cmd) => cmd.run().await,
        }
    }
}
