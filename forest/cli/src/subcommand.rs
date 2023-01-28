// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::cli::{Config, Subcommand};

/// Process CLI sub-command
pub(super) async fn process(command: Subcommand, config: Config) -> anyhow::Result<()> {
    // Run command
    match command {
        Subcommand::Fetch(cmd) => cmd.run(config).await,
        Subcommand::Chain(cmd) => cmd.chain_commands.run(config).await,
        Subcommand::Auth(cmd) => cmd.auth_commands.run(config).await,
        Subcommand::Net(cmd) => cmd.net_commands.run(config).await,
        Subcommand::Wallet(cmd) => cmd.wallet_commands.run(config).await,
        Subcommand::Sync(cmd) => cmd.sync_commands.run(config).await,
        Subcommand::Mpool(cmd) => cmd.mpool_commands.run(config),
        Subcommand::State(cmd) => cmd.state_commands.run(config),
        Subcommand::Config(cmd) => cmd.config_commands.run(&config, &mut std::io::stdout()),
        Subcommand::Send(cmd) => cmd.run(config).await,
        Subcommand::DB(cmd) => cmd.db_commands.run(&config),
        Subcommand::Snapshot(cmd) => cmd.snapshot_commands.run(config).await,
    }
}
