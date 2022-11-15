// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::cli::{Config, Subcommand};

/// Process CLI sub-command
pub(super) async fn process(command: Subcommand, config: Config) {
    // Run command
    match command {
        Subcommand::Fetch(cmd) => {
            cmd.run(config).await;
        }
        Subcommand::Chain(cmd) => {
            cmd.run(config).await;
        }
        Subcommand::Auth(cmd) => {
            cmd.run(config).await;
        }
        Subcommand::Genesis(cmd) => {
            cmd.run().await;
        }
        Subcommand::Net(cmd) => {
            cmd.run(config).await;
        }
        Subcommand::Wallet(cmd) => {
            cmd.run(config).await;
        }
        Subcommand::Sync(cmd) => {
            cmd.run(config).await;
        }
        Subcommand::Mpool(cmd) => {
            cmd.run(config).await;
        }
        Subcommand::State(cmd) => {
            cmd.run(config).await;
        }
        Subcommand::Config(cmd) => {
            cmd.run(&config, &mut std::io::stdout()).await;
        }
        Subcommand::Send(cmd) => {
            cmd.run(config).await.unwrap();
        }
        Subcommand::Snapshot(cmd) => cmd.run(config).await,
        Subcommand::DB(cmd) => cmd.run(&config).await,
    }
}
