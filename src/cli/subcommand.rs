// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

use crate::cli_shared::cli::CliOpts;
use crate::networks::ChainConfig;
use crate::rpc_client::chain_get_name;

use super::cli::{Config, Subcommand};

/// Process CLI sub-command
pub async fn process(
    command: Subcommand,
    mut config: Config,
    opts: &CliOpts,
) -> anyhow::Result<()> {
    if opts.chain.is_none() {
        if let Ok(name) = chain_get_name((), &config.client.rpc_token).await {
            if name == "calibnet" {
                config.chain = Arc::new(ChainConfig::calibnet());
            }
        }
    }
    if config.chain.is_testnet() {
        crate::shim::address::set_current_network(crate::shim::address::Network::Testnet);
    }
    // Run command
    match command {
        Subcommand::Fetch(cmd) => cmd.run(config).await,
        Subcommand::Chain(cmd) => cmd.run(config).await,
        Subcommand::Auth(cmd) => cmd.run(config).await,
        Subcommand::Net(cmd) => cmd.run(config).await,
        Subcommand::Wallet(cmd) => cmd.run(config).await,
        Subcommand::Sync(cmd) => cmd.run(config).await,
        Subcommand::Mpool(cmd) => cmd.run(config),
        Subcommand::State(cmd) => cmd.run(config).await,
        Subcommand::Config(cmd) => cmd.run(&config, &mut std::io::stdout()),
        Subcommand::Send(cmd) => cmd.run(config).await,
        Subcommand::Info(cmd) => cmd.run(config, opts).await,
        Subcommand::DB(cmd) => cmd.run(&config).await,
        Subcommand::Snapshot(cmd) => cmd.run(config).await,
        Subcommand::Attach(cmd) => cmd.run(config),
        Subcommand::Shutdown(cmd) => cmd.run(config).await,
    }
}
