// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::cli::{Config, Subcommand};

/// Process CLI sub-command
pub(super) async fn process(command: Subcommand, config: Config) -> anyhow::Result<()> {
    if config.chain.name == "calibnet" {
        forest_shim::address::set_current_network(forest_shim::address::Network::Testnet);
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
        Subcommand::State(cmd) => cmd.run(config),
        Subcommand::Config(cmd) => cmd.run(&config, &mut std::io::stdout()),
        Subcommand::Send(cmd) => cmd.run(config).await,
        Subcommand::DB(cmd) => cmd.run(&config),
        Subcommand::Snapshot(cmd) => cmd.run(config).await,
        Subcommand::Attach(cmd) => cmd.run(config),
        Subcommand::Shutdown(cmd) => cmd.run(config).await,
    }
}
