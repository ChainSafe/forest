// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::str::FromStr as _;

use anyhow::Context as _;
use clap::{Parser, Subcommand};
use forest::interop_tests_private::networks::NetworkChain;
use forest::interop_tests_private::rpc::{self, prelude::*};
use forest::interop_tests_private::shim::address::{CurrentNetwork, Network};
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(
    name = "forest-wallet-tests",
    about = "Calibnet wallet integration tests for Forest",
    version
)]
struct Cli {
    #[command(subcommand)]
    cmd: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Run the basic wallet check.
    Basic,
    /// Run the delegated wallet check.
    Delegated,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    // Parse args first so `--help` and `--version` don't require the daemon
    // to be reachable.
    let Cli { cmd } = Cli::parse();

    init_tracing();

    // Detect testnet vs mainnet so address parsing accepts the expected `t…` / `f…` prefixes.
    set_network_from_daemon().await?;

    match cmd {
        Command::Basic => forest_wallet_tests::scenarios::basic::run().await,
        Command::Delegated => forest_wallet_tests::scenarios::delegated::run().await,
    }
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .try_init();
}

async fn set_network_from_daemon() -> anyhow::Result<()> {
    let client = rpc::Client::default_or_from_env(None)
        .context("could not create RPC client (is FULLNODE_API_INFO set?)")?;
    let name = StateNetworkName::call(&client, ()).await?;
    let chain = NetworkChain::from_str(&name)?;
    if chain.is_testnet() {
        CurrentNetwork::set_global(Network::Testnet);
    }
    Ok(())
}
